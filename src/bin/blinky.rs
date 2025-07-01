#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed, Pull};
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::usart::{Config, Uart};
use embassy_stm32::adc::{Adc, Resolution, SampleTime};
use embassy_stm32::peripherals;
use embassy_time::{Timer, Instant};
use embassy_sync::signal::Signal;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use heapless::String;
use embassy_stm32::bind_interrupts;
use {defmt_rtt as _, panic_probe as _};

// BUTTON_SIGNAL é um sinal para notificar eventos de botão
static BUTTON_SIGNAL: Signal<ThreadModeRawMutex, ()> = Signal::new(); 

// Estrutura para armazenar estatísticas do sistema
#[derive(Clone, Copy)]
struct TaskStats {
    task_count: u32,
    uptime_ms: u64,
    button_presses: u32,
    led1_blinks: u32,
    led2_blinks: u32,
    adc_samples: u32,
    posicao: u32,
}

static mut SYSTEM_STATS: TaskStats = TaskStats {
    task_count: 0,
    uptime_ms: 0,
    button_presses: 0,
    led1_blinks: 0,
    led2_blinks: 0,
    adc_samples: 0,
    posicao: 0,
};

static mut LEDSPEED: u32 = 0;

bind_interrupts!(struct Irqs {
    USART1 => embassy_stm32::usart::InterruptHandler<peripherals::USART1>;
});

#[embassy_executor::task]
async fn console_shell(mut uart: Uart<'static, embassy_stm32::mode::Async>) {
    info!("Console_Shell iniciado");
    
    let mut buffer = [0u8; 1];
    let mut _cmd_buffer = String::<64>::new();
    loop {
        let _ = uart.read(&mut buffer).await; 
        if buffer[0] == b'\r' || buffer[0] == b'\n' {
            if !_cmd_buffer.is_empty() {
                process_command(&mut uart, &_cmd_buffer).await;
                _cmd_buffer.clear();
            }
            let _ = uart.write(b"\r\n> ").await; // Prompt
        } else if buffer[0] == b'\x08' || buffer[0] == 127 { // Backspace
            if !_cmd_buffer.is_empty() {
                _cmd_buffer.pop();
                let _ = uart.write(b"\x08 \x08").await; // Apaga o último caractere
            }
        } else {
            if _cmd_buffer.push(buffer[0] as char).is_ok() {
                let _ = uart.write(&buffer).await; // Ecoa o caractere
            }
        }
    }
}


async fn process_command(uart: &mut Uart<'static, embassy_stm32::mode::Async>, cmd: &str) {
    let mut response = String::<512>::new();
    info!("Mensagem: {}", cmd);
    
    if cmd.starts_with("led1=") {
        if let Ok(valor) = cmd[5..].trim().parse::<u32>() {
            unsafe { LEDSPEED = valor; }
            let _ = core::fmt::write(&mut response, format_args!("Velocidade do LED1 ajustada para {} ms\r\n", valor));
        } else {
            let _ = core::fmt::write(&mut response, format_args!("Valor inválido!\r\n"));
        }
    }else{
            match cmd.trim() {
            "help" => {
            let _ = core::fmt::write(&mut response, format_args!(
                "\n=== Comandos do Sistema ===\n\
                 status\n\r\
                 reset\n\r\
                 help\n\r\
                 led1=n (n velocidade desejada em ms)\n\r"
            ));
            }
            "status" => {
                let stats = unsafe { SYSTEM_STATS };
                let _ = core::fmt::write(&mut response, format_args!(
                "\n=== Status do Sistema ===\r\n\
                 Uptime: {} ms\r\n\
                 Tarefas ativas: {}\r\n\
                 Botão pressionado: {} vezes\r\n\
                 LED1 piscou: {} vezes\r\n\
                 LED2 piscou: {} vezes\r\n\
                 ADC Samples: {} vezes\r\n\
                 Posição do peso: {}\r\n",
                stats.uptime_ms, stats.task_count, 
                stats.button_presses, stats.led1_blinks, stats.led2_blinks,
                stats.adc_samples, stats.posicao
            ));
            }
            "reset" => {
                unsafe {
                    SYSTEM_STATS.button_presses = 0;
                    SYSTEM_STATS.led1_blinks = 0;
                    SYSTEM_STATS.led2_blinks = 0;
                }
                let _ = core::fmt::write(&mut response, format_args!("\nEstatísticas resetadas!\r\n"));
            }
        
            _ => {
                let _ = core::fmt::write(&mut response, format_args!("Comando não encontrado. Digite 'help' para ajuda.\r\n"));
            }
        }

    }
    
    let _ = uart.write(response.as_bytes()).await;
    let _ = uart.write(b"\r\n> ").await;
}

const PESOS: [u32; 8] = [0, 1000, 2000, 3000, 4000, 5000, 6000, 7000];

fn calcula_posicao_peso(sensores: &[u16; 8]) -> u32 {
    let mut soma_pesos = 0u32;
    let mut soma_valores = 0u32;

    for (i, &valor) in sensores.iter().enumerate() {
        soma_pesos += valor as u32 * PESOS[i];
        soma_valores += valor as u32;
    }
    if soma_valores == 0 {
        0
    } else {
        soma_pesos / soma_valores
    }
}

#[embassy_executor::task]
async fn adc_task(
    mut adc: Adc<'static, peripherals::ADC1>,
    mut pin0: peripherals::PA0,
    mut pin1: peripherals::PA1,
    mut pin2: peripherals::PA2,
    mut pin3: peripherals::PA3,
    mut pin4: peripherals::PA4,
    mut pin5: peripherals::PA5,
    mut pin6: peripherals::PA6,
    mut pin7: peripherals::PA7,
) {
    adc.set_resolution(Resolution::BITS12);
    adc.set_sample_time(SampleTime::CYCLES3);

    loop {
        let mut samples = [0u16; 8];
        samples[0] = adc.blocking_read(&mut pin0);
        samples[1] = adc.blocking_read(&mut pin1);
        samples[2] = adc.blocking_read(&mut pin2);
        samples[3] = adc.blocking_read(&mut pin3);
        samples[4] = adc.blocking_read(&mut pin4);
        samples[5] = adc.blocking_read(&mut pin5);
        samples[6] = adc.blocking_read(&mut pin6);
        samples[7] = adc.blocking_read(&mut pin7);

        let pos = calcula_posicao_peso(&samples);

        unsafe {
            SYSTEM_STATS.adc_samples += 1;
            SYSTEM_STATS.posicao = pos;
        }

        Timer::after_micros(100).await;
    }
}

#[embassy_executor::task]
async fn blink_fast(mut led: Output<'static>) {
    loop {
        led.set_high();
        Timer::after_millis(200).await;
        led.set_low();
        Timer::after_millis(200).await;
        unsafe {
            SYSTEM_STATS.led1_blinks += 1;
        }
    }
}

#[embassy_executor::task]
async fn blink_slow(mut led: Output<'static>) {
    loop {
        led.set_high();
        Timer::after_millis(1000).await;
        led.set_low();
        Timer::after_millis(1000).await;
        unsafe {
            SYSTEM_STATS.led2_blinks += 1;
        }
    }
}

#[embassy_executor::task]
async fn button_handler(mut button: ExtiInput<'static>) {
    loop {
        button.wait_for_rising_edge().await;
        info!("Botão pressionado!");
        unsafe {
            SYSTEM_STATS.button_presses += 1;
        }
        BUTTON_SIGNAL.signal(());
        Timer::after_millis(100).await; 
    }
}

#[embassy_executor::task]
async fn system_monitor() {
    let start_time = Instant::now();
    loop {
        Timer::after_secs(1).await;
        info!("------------------------------");
        unsafe {
            SYSTEM_STATS.uptime_ms = start_time.elapsed().as_millis();
            SYSTEM_STATS.task_count = 4; 
        }
        let uptime_s = unsafe { SYSTEM_STATS.uptime_ms / 1000 };
        info!("Monitor: Sistema ativo há {}s, {} amostras ADC", 
              uptime_s,
              unsafe { SYSTEM_STATS.adc_samples });
        info!("Performance: {} amostras/s, {} botão, LEDs: {}/{}, Posicao: {}", 
              if uptime_s > 0 { unsafe { SYSTEM_STATS.adc_samples as u64 * 1000 / SYSTEM_STATS.uptime_ms } } else { 0 },
              unsafe { SYSTEM_STATS.button_presses },
              unsafe { SYSTEM_STATS.led1_blinks },
              unsafe { SYSTEM_STATS.led2_blinks },
              unsafe { SYSTEM_STATS.posicao });
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    let led1 = Output::new(p.PC13, Level::Low, Speed::Low);
    let led2 = Output::new(p.PA11, Level::Low, Speed::Low);
    let button = ExtiInput::new(p.PB12, p.EXTI12, Pull::Down);
    let adc = Adc::new(p.ADC1);

    let mut config = Config::default();
    config.baudrate = 9600; 
    let uart = Uart::new(
        p.USART1,
        p.PA10, 
        p.PA9,
        Irqs,
        p.DMA2_CH7, 
        p.DMA2_CH2, 
        config,
    ).unwrap();

    spawner.spawn(console_shell(uart)).unwrap();
    spawner.spawn(blink_fast(led1)).unwrap();
    spawner.spawn(blink_slow(led2)).unwrap();
    spawner.spawn(button_handler(button)).unwrap();
    spawner.spawn(adc_task(
        adc, p.PA0, p.PA1, p.PA2, p.PA3, 
        p.PA4, p.PA5, p.PA6, p.PA7
    )).unwrap();
    spawner.spawn(system_monitor()).unwrap();

    loop {
        Timer::after_secs(10).await;
    }
}