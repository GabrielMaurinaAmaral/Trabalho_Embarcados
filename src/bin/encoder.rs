#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Input, Pull, Level};
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::{peripherals, Config};
use embassy_stm32::time::Hertz;
use embassy_time::{Duration, Timer};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::mutex::Mutex;
use core::sync::atomic::{AtomicU32, Ordering};
use {defmt_rtt as _, panic_probe as _};

// Contador global thread-safe usando AtomicU32
static ENCODER_COUNTER: AtomicU32 = AtomicU32::new(0);

// Alternativa usando Mutex para operações mais complexas
static ENCODER_DATA: Mutex<ThreadModeRawMutex, EncoderData> = 
    Mutex::new(EncoderData::new());

#[derive(Clone, Copy)]
struct EncoderData {
    count: u32,
    direction: Direction,
    last_timestamp: u64,
    rpm: f32,
}

#[derive(Clone, Copy, defmt::Format, PartialEq)]
enum Direction {
    Forward,
    Reverse,
    Unknown,
}

impl EncoderData {
    const fn new() -> Self {
        Self {
            count: 0,
            direction: Direction::Unknown,
            last_timestamp: 0,
            rpm: 0.0,
        }
    }
}

// Remover bind_interrupts - não é necessário para este caso

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Configuração do sistema
    let mut config = Config::default();
    {
        use embassy_stm32::rcc::*;
        config.rcc.hse = Some(Hse {
            freq: Hertz(25_000_000),
            mode: HseMode::Oscillator,
        });
        config.rcc.pll_src = PllSource::HSE;
        config.rcc.pll = Some(Pll {
            prediv: PllPreDiv::DIV25,
            mul: PllMul::MUL192,
            divp: Some(PllPDiv::DIV2), // 96 MHz
            divq: Some(PllQDiv::DIV4), // 48 MHz
            divr: None,
        });
        config.rcc.ahb_pre = AHBPrescaler::DIV1;
        config.rcc.apb1_pre = APBPrescaler::DIV2;
        config.rcc.apb2_pre = APBPrescaler::DIV1;
        config.rcc.sys = Sysclk::PLL1_P;
    }

    let p = embassy_stm32::init(config);
    
    info!("Sistema inicializado - Contador Encoder Magnético");

    // Spawn das tarefas
    spawner.spawn(encoder_task(p.PA12, p.EXTI12)).unwrap();
    spawner.spawn(display_task()).unwrap();
    spawner.spawn(rpm_calculator_task()).unwrap();

    // Task principal - pode implementar outras funcionalidades
    loop {
        Timer::after(Duration::from_secs(10)).await;
        info!("Sistema rodando normalmente...");
    }
}

#[embassy_executor::task]
async fn encoder_task(pin: peripherals::PA12, exti: peripherals::EXTI12) {
    // Configura PA12 como entrada com pull-up interno
    let input = Input::new(pin, Pull::Up);
    let mut exti_input = ExtiInput::new(input, exti);
    
    info!("Encoder configurado no pino PA12");
    info!("Aguardando pulsos do encoder...");

    let mut last_level = Level::High;
    let _pulses_per_revolution = 20; // Ajuste conforme seu encoder
    
    loop {
        // Aguarda por mudança de estado (borda de subida ou descida)
        exti_input.wait_for_any_edge().await;
        
        let current_time = embassy_time::Instant::now().as_micros();
        let current_level = if exti_input.is_high() { Level::High } else { Level::Low };
        
        // Processa apenas bordas de descida para evitar dupla contagem
        if last_level == Level::High && current_level == Level::Low {
            // Incrementa contador atômico
            let new_count = ENCODER_COUNTER.fetch_add(1, Ordering::SeqCst) + 1;
            
            // Atualiza dados mais complexos usando mutex
            {
                let mut data = ENCODER_DATA.lock().await;
                data.count = new_count;
                
                // Calcula direção baseado no tempo entre pulsos
                let time_diff = current_time - data.last_timestamp;
                if time_diff > 0 {
                    // Simples detecção de direção baseada na frequência
                    // Para encoder com direção real, você precisaria de um segundo canal
                    data.direction = if time_diff < 50000 { // < 50ms = alta velocidade
                        Direction::Forward
                    } else {
                        Direction::Reverse
                    };
                }
                
                data.last_timestamp = current_time;
            }
            
            defmt::trace!("Pulso detectado: {}", new_count);
        }
        
        last_level = current_level;
        
        // Pequeno delay para debounce
        Timer::after(Duration::from_micros(500)).await;
    }
}

#[embassy_executor::task]
async fn display_task() {
    let mut last_count = 0u32;
    
    loop {
        Timer::after(Duration::from_secs(1)).await;
        
        let current_count = ENCODER_COUNTER.load(Ordering::SeqCst);
        
        if current_count != last_count {
            let data = ENCODER_DATA.lock().await;
            
            info!("=== Status do Encoder ===");
            info!("Contagem total: {}", data.count);
            info!("Direção: {}", data.direction);
            info!("RPM estimado: {}", data.rpm as u32);
            info!("Pulsos/seg: {}", current_count - last_count);
            info!("========================");
            
            last_count = current_count;
        }
    }
}

#[embassy_executor::task]
async fn rpm_calculator_task() {
    let mut last_count = 0u32;
    let mut last_time = embassy_time::Instant::now();
    let pulses_per_revolution = 9.0; // Ajuste conforme seu encoder
    
    loop {
        Timer::after(Duration::from_millis(250)).await; // Atualiza RPM a cada 250ms
        
        let current_count = ENCODER_COUNTER.load(Ordering::SeqCst);
        let current_time = embassy_time::Instant::now();
        
        let pulse_diff = current_count - last_count;
        let time_diff_secs = current_time.duration_since(last_time).as_millis() as f32 / 1000.0;
        
        if pulse_diff > 0 && time_diff_secs > 0.0 {
            let pulses_per_sec = pulse_diff as f32 / time_diff_secs;
            let revolutions_per_sec = pulses_per_sec / pulses_per_revolution;
            let rpm = revolutions_per_sec * 60.0;
            
            // Atualiza RPM nos dados compartilhados
            {
                let mut data = ENCODER_DATA.lock().await;
                data.rpm = rpm;
            }
        }
        
        last_count = current_count;
        last_time = current_time;
    }
}

// Função auxiliar para acessar o contador de forma thread-safe
pub fn get_encoder_count() -> u32 {
    ENCODER_COUNTER.load(Ordering::SeqCst)
}

// Função auxiliar para resetar o contador
pub fn reset_encoder_count() {
    ENCODER_COUNTER.store(0, Ordering::SeqCst);
}

// Função auxiliar para obter dados completos do encoder
pub async fn get_encoder_data() -> EncoderData {
    *ENCODER_DATA.lock().await
}
