#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed, Pull};
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::usart::{Config, Uart};
use embassy_stm32::bind_interrupts;
use embassy_stm32::peripherals;
use embassy_time::{Timer, Instant};
use embassy_sync::signal::Signal;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use heapless::String;
use {defmt_rtt as _, panic_probe as _};

static BUTTON_SIGNAL: Signal<ThreadModeRawMutex, ()> = Signal::new();
//static CONSOLE_OUTPUT: Signal<ThreadModeRawMutex, String<256>> = Signal::new();

#[derive(Clone, Copy)]
struct TaskStats {
    task_count: u32,
    uptime_ms: u64,
    button_presses: u32,
    led1_blinks: u32,
    led2_blinks: u32,
}

static mut SYSTEM_STATS: TaskStats = TaskStats {
    task_count: 0,
    uptime_ms: 0,
    button_presses: 0,
    led1_blinks: 0,
    led2_blinks: 0,
};
bind_interrupts!(struct Irqs {
    USART1 => embassy_stm32::usart::InterruptHandler<peripherals::USART1>;
});

#[embassy_executor::task]
async fn console_shell(mut uart: Uart<'static, embassy_stm32::mode::Async>) {
    info!("Console_Shell iniciado");
    
    let mut buffer = [0u8; 64];
    let mut cmd_buffer = String::<64>::new();
    
    let welcome = b"\r\nEmbassy STM32 System Console\r\nDigite 'help' para ver comandos disponiveis\r\n> ";
    let _ = uart.write(welcome).await;
    
    loop {
        match uart.read(&mut buffer[..1]).await {
            Ok(_) => {
               
            }
            Err(_) => {

            }
        }
    }
}

async fn process_command(uart: &mut Uart<'static, embassy_stm32::mode::Async>, cmd: &str) {
    let mut response = String::<512>::new();
    
    match cmd.trim() {
        "help" => {
            let _ = core::fmt::write(&mut response, format_args!(
                "Comandos disponveis:\r\n\
                 help        - Mostra esta ajuda\r\n\
                 status      - Informacoes do sistema\r\n\
                 tasks       - Lista tarefas ativas\r\n\
                 heap        - Informacoes de memoria\r\n\
                 runtime     - Estatsticas de runtime\r\n\
                 realtime    - Informacoes de tarefas tempo real\r\n\
                 reset       - Reset estatsticas\r\n"
            ));
        }
        _ => {
            let _ = core::fmt::write(&mut response, format_args!("Comando não encontrado. Digite 'help' para ajuda.\r\n"));
        }
    }
    
    let _ = uart.write(response.as_bytes()).await;
}

#[embassy_executor::task]
async fn blink_fast(mut led: Output<'static>) {
    loop {
        led.set_high();
        info!("Led 1 aceso!");
        Timer::after_millis(200).await;
        led.set_low();
        info!("Led 1 apagado!");
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
        info!("Led 2 aceso!");
        Timer::after_millis(1000).await;
        led.set_low();
        info!("Led 2 apagado!");
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
        
        Timer::after_millis(50).await; 
    }
}

#[embassy_executor::task]
async fn system_monitor() {
    let start_time = Instant::now();
    
    loop {
        Timer::after_secs(1).await;
        
        // Atualizar uptime
        unsafe {
            SYSTEM_STATS.uptime_ms = start_time.elapsed().as_millis();
            SYSTEM_STATS.task_count = 6; // Número fixo de tarefas
        }
        
        info!("Monitor: Sistema rodando há {} ms", unsafe { SYSTEM_STATS.uptime_ms });
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    info!("Sistema com múltiplos LEDs, botão e console inicializado!");

    let led1 = Output::new(p.PC13, Level::Low, Speed::Low);
    let led2 = Output::new(p.PA11, Level::Low, Speed::Low);
    let button = ExtiInput::new(p.PB12, p.EXTI12, Pull::Down);
    
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
    
    spawner.spawn(blink_fast(led1)).unwrap();
    spawner.spawn(blink_slow(led2)).unwrap();
    spawner.spawn(button_handler(button)).unwrap();
    spawner.spawn(console_shell(uart)).unwrap();
    spawner.spawn(system_monitor()).unwrap();
    
    info!("Todas as tarefas inicializadas!");
    
    loop {
        Timer::after_secs(10).await;
        info!("Sistema em operação normal - {} tarefas ativas", unsafe { SYSTEM_STATS.task_count });
    }
}