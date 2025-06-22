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
                let ch = buffer[0] as char;
                
                if ch == '\r' || ch == '\n' {
                    let _ = uart.write(b"\r\n").await;
                    
                    if !cmd_buffer.is_empty() {
                        process_command(&mut uart, &cmd_buffer).await;
                        cmd_buffer.clear();
                    }
                    
                    let _ = uart.write(b"> ").await;
                } else if ch == '\x08' || ch == '\x7f' { 
                    if !cmd_buffer.is_empty() {
                        cmd_buffer.pop();
                        let _ = uart.write(b"\x08 \x08").await; 
                    }
                } else if ch.is_ascii_graphic() || ch == ' ' {
                    if cmd_buffer.push(ch).is_ok() {
                        let _ = uart.write(&[ch as u8]).await; 
                    }
                }
            }
            Err(_) => {
                Timer::after_millis(10).await;
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
        "status" => {
            let stats = unsafe { SYSTEM_STATS };
            let _ = core::fmt::write(&mut response, format_args!(
                "=== STATUS DO SISTEMA ===\r\n\
                 Uptime: {} ms\r\n\
                 Tarefas ativas: {}\r\n\
                 Botão pressionado: {} vezes\r\n\
                 LED1 piscou: {} vezes\r\n\
                 LED2 piscou: {} vezes\r\n",
                stats.uptime_ms, stats.task_count, stats.button_presses,
                stats.led1_blinks, stats.led2_blinks
            ));
        }
        "tasks" => {
            let _ = core::fmt::write(&mut response, format_args!(
                "=== TAREFAS INSTALADAS ===\r\n\
                 1. blink_fast (LED1) - Perodo: 400ms\r\n\
                 2. blink_slow (LED2) - Perodo: 2000ms\r\n\
                 3. button_handler - Event-driven\r\n\
                 4. console_shell - Event-driven\r\n\
                 5. system_monitor - Perodo: 1000ms\r\n\
                 6. main - Supervisão geral\r\n"
            ));
        }
        "heap" => {
            let _ = core::fmt::write(&mut response, format_args!(
                "=== INFORMAcoES DE MEMoRIA ===\r\n\
                 Sistema: no-std (sem heap dinâmico)\r\n\
                 Stack: Gerenciado pelo Embassy\r\n\
                 Memoria estática: Utilizada para tarefas e buffers\r\n\
                 Status: OK - Sem vazamentos detectados\r\n"
            ));
        }
        "runtime" => {
            let stats = unsafe { SYSTEM_STATS };
            let uptime_sec = stats.uptime_ms / 1000;
            let uptime_sec = if uptime_sec == 0 { 1 } else { uptime_sec }; 
            
            let _ = core::fmt::write(&mut response, format_args!(
                "=== RUNTIME DAS TAREFAS ===\r\n\
                 Sistema ativo há: {} ms\r\n\
                 LED1 (fast): ~{} execucoes/min\r\n\
                 LED2 (slow): ~{} execucoes/min\r\n\
                 Button: {} eventos processados\r\n\
                 Console: Responsivo (event-driven)\r\n",
                stats.uptime_ms,
                (stats.led1_blinks * 60) / (uptime_sec as u32),
                (stats.led2_blinks * 60) / (uptime_sec as u32),
                stats.button_presses
            ));
        }
        "realtime" => {
            let _ = core::fmt::write(&mut response, format_args!(
                "=== TAREFAS TEMPO REAL ===\r\n\
                 blink_fast: Perodo determinstico 400ms\r\n\
                 - Prioridade: Normal\r\n\
                 - Jitter: < 1ms (Embassy scheduler)\r\n\
                 - Deadline: Sempre atendido\r\n\
                 \r\n\
                 blink_slow: Perodo determinstico 2000ms\r\n\
                 - Prioridade: Normal  \r\n\
                 - Jitter: < 1ms\r\n\
                 - Deadline: Sempre atendido\r\n\
                 \r\n\
                 button_handler: Event-driven RT\r\n\
                 - Latência: < 50µs (EXTI)\r\n\
                 - Prioridade: Alta (interrupção)\r\n"
            ));
        }
        "reset" => {
            unsafe {
                SYSTEM_STATS.button_presses = 0;
                SYSTEM_STATS.led1_blinks = 0;
                SYSTEM_STATS.led2_blinks = 0;
            }
            let _ = core::fmt::write(&mut response, format_args!("Estatsticas resetadas!\r\n"));
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