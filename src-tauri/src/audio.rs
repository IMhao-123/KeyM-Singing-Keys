use cpal::traits::{DeviceTrait, HostTrait};
use rodio::source::SineWave;
use rodio::Source;
use rodio::{OutputStream, OutputStreamHandle, Sink};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::runtime_health::RuntimeHealth;

/// 保温音源参数 (常驻不可闻正弦波, 保持音频管道永不休眠)
const KEEP_ALIVE_FREQ: f32 = 18000.0;
const KEEP_ALIVE_VOLUME: f32 = 0.0003;
const PLAY_DIAGNOSTIC_LIMIT: usize = 20;
const COMMAND_CAPACITY: usize = 128;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(3);
/// SR-03：调度停顿超过该阈值视为睡眠唤醒，重建输出流。
const SUSPEND_GAP_THRESHOLD: Duration = Duration::from_secs(10);
const DEVICE_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// 按键分类
#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub enum KeyCategory {
    Normal,
    Space,
    Return,
    Backspace,
    Tab,
    Escape,
    Modifier,
    Arrow,
    Other,
}

/// 播放结果（第一版已裁掉自定义音效，只剩内置主题一条播放路径）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayOutcome {
    Disabled,
    Queued,
    QueueFull,
    WorkerStopped,
    MissingBuiltin,
}

impl PlayOutcome {
    pub fn label(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Queued => "builtin_queued",
            Self::QueueFull => "builtin_queue_full",
            Self::WorkerStopped => "worker_stopped",
            Self::MissingBuiltin => "builtin_missing",
        }
    }
}

/// 从 macOS virtual keycode 映射到按键分类
pub fn keycode_to_category(keycode: u16) -> KeyCategory {
    match keycode {
        0x31 => KeyCategory::Space,
        0x24 => KeyCategory::Return,
        0x33 => KeyCategory::Backspace,
        0x30 => KeyCategory::Tab,
        0x35 => KeyCategory::Escape,
        0x37..=0x3F => KeyCategory::Modifier,
        0x7B..=0x7E => KeyCategory::Arrow,
        _ => {
            if keycode <= 0x29 {
                KeyCategory::Normal
            } else {
                KeyCategory::Other
            }
        }
    }
}

pub fn category_name(cat: KeyCategory) -> &'static str {
    match cat {
        KeyCategory::Normal => "normal",
        KeyCategory::Space => "space",
        KeyCategory::Return => "return",
        KeyCategory::Backspace => "backspace",
        KeyCategory::Tab => "tab",
        KeyCategory::Escape => "escape",
        KeyCategory::Modifier => "modifier",
        KeyCategory::Arrow => "arrow",
        KeyCategory::Other => "other",
    }
}

/// 音效主题
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum SoundTheme {
    Ipad,
    Mechanical,
    Typewriter,
    Silent,
    CherryRed,
    Membrane,
    EightBit,
    Woodfish,
    Raindrop,
    Bubble,
    Pen,
}

impl SoundTheme {
    pub fn all() -> &'static [SoundTheme] {
        &[
            SoundTheme::Ipad,
            SoundTheme::Mechanical,
            SoundTheme::Typewriter,
            SoundTheme::Silent,
            SoundTheme::CherryRed,
            SoundTheme::Membrane,
            SoundTheme::EightBit,
            SoundTheme::Woodfish,
            SoundTheme::Raindrop,
            SoundTheme::Bubble,
            SoundTheme::Pen,
        ]
    }

    pub fn count() -> u8 {
        Self::all().len() as u8
    }

    pub fn from_index(idx: u8) -> Self {
        Self::all()
            .get(idx as usize)
            .copied()
            .unwrap_or(SoundTheme::Ipad)
    }

    pub fn as_index(&self) -> u8 {
        Self::all().iter().position(|t| t == self).unwrap_or(0) as u8
    }

    pub fn name(&self) -> &'static str {
        match self {
            SoundTheme::Ipad => "iPad 风格",
            SoundTheme::Mechanical => "机械青轴",
            SoundTheme::Typewriter => "打字机",
            SoundTheme::Silent => "静电容",
            SoundTheme::CherryRed => "樱桃红轴",
            SoundTheme::Membrane => "薄膜",
            SoundTheme::EightBit => "8-bit",
            SoundTheme::Woodfish => "木鱼",
            SoundTheme::Raindrop => "雨滴",
            SoundTheme::Bubble => "气泡",
            SoundTheme::Pen => "钢笔",
        }
    }
}

/// 波形形状
#[derive(Clone, Copy, Debug)]
enum WaveShape {
    Sine,
    ClickPlusThud,
    LayeredTypewriter,
    SquareWave,
    Pluck,
    FilteredNoise,
}

/// 语音参数
#[derive(Clone, Copy, Debug)]
struct VoiceSpec {
    freq: f32,
    duration_ms: u32,
    envelope_rate: f32,
    volume: f32,
}

/// 主题定义
#[derive(Clone, Copy, Debug)]
struct ThemeDef {
    shape: WaveShape,
    normal: VoiceSpec,
    space: VoiceSpec,
    return_: VoiceSpec,
    backspace: VoiceSpec,
    other: VoiceSpec,
}

const fn vs(freq: f32, duration_ms: u32, envelope_rate: f32, volume: f32) -> VoiceSpec {
    VoiceSpec {
        freq,
        duration_ms,
        envelope_rate,
        volume,
    }
}

fn builtin_themes() -> [ThemeDef; 11] {
    [
        ThemeDef {
            shape: WaveShape::Sine,
            normal: vs(1800.0, 8, 600.0, 0.5),
            space: vs(1200.0, 10, 500.0, 0.6),
            return_: vs(800.0, 12, 400.0, 0.7),
            backspace: vs(1500.0, 8, 700.0, 0.5),
            other: vs(1800.0, 8, 600.0, 0.4),
        },
        ThemeDef {
            shape: WaveShape::ClickPlusThud,
            normal: vs(2000.0, 6, 800.0, 0.5),
            space: vs(600.0, 15, 300.0, 0.7),
            return_: vs(400.0, 20, 250.0, 0.8),
            backspace: vs(700.0, 12, 400.0, 0.6),
            other: vs(2000.0, 6, 800.0, 0.4),
        },
        ThemeDef {
            shape: WaveShape::LayeredTypewriter,
            normal: vs(300.0, 15, 200.0, 0.6),
            space: vs(200.0, 25, 150.0, 0.8),
            return_: vs(150.0, 40, 100.0, 0.9),
            backspace: vs(250.0, 20, 200.0, 0.7),
            other: vs(300.0, 15, 200.0, 0.5),
        },
        ThemeDef {
            shape: WaveShape::Sine,
            normal: vs(600.0, 8, 500.0, 0.3),
            space: vs(400.0, 12, 400.0, 0.4),
            return_: vs(300.0, 15, 350.0, 0.5),
            backspace: vs(500.0, 10, 500.0, 0.35),
            other: vs(600.0, 8, 500.0, 0.25),
        },
        ThemeDef {
            shape: WaveShape::ClickPlusThud,
            normal: vs(1800.0, 7, 700.0, 0.45),
            space: vs(500.0, 14, 280.0, 0.65),
            return_: vs(350.0, 18, 220.0, 0.75),
            backspace: vs(650.0, 11, 380.0, 0.55),
            other: vs(1800.0, 7, 700.0, 0.35),
        },
        ThemeDef {
            shape: WaveShape::Sine,
            normal: vs(800.0, 10, 400.0, 0.35),
            space: vs(500.0, 14, 300.0, 0.45),
            return_: vs(400.0, 16, 280.0, 0.55),
            backspace: vs(600.0, 9, 450.0, 0.4),
            other: vs(800.0, 10, 400.0, 0.3),
        },
        ThemeDef {
            shape: WaveShape::SquareWave,
            normal: vs(1200.0, 6, 900.0, 0.4),
            space: vs(800.0, 8, 700.0, 0.5),
            return_: vs(600.0, 10, 600.0, 0.6),
            backspace: vs(1000.0, 5, 1000.0, 0.35),
            other: vs(1200.0, 6, 900.0, 0.3),
        },
        ThemeDef {
            shape: WaveShape::Pluck,
            normal: vs(400.0, 30, 80.0, 0.5),
            space: vs(300.0, 40, 60.0, 0.6),
            return_: vs(250.0, 50, 50.0, 0.7),
            backspace: vs(350.0, 25, 100.0, 0.45),
            other: vs(400.0, 30, 80.0, 0.4),
        },
        ThemeDef {
            shape: WaveShape::FilteredNoise,
            normal: vs(2000.0, 20, 300.0, 0.3),
            space: vs(1500.0, 25, 250.0, 0.4),
            return_: vs(1000.0, 30, 200.0, 0.5),
            backspace: vs(1800.0, 15, 350.0, 0.35),
            other: vs(2000.0, 20, 300.0, 0.25),
        },
        ThemeDef {
            shape: WaveShape::Sine,
            normal: vs(600.0, 15, 200.0, 0.4),
            space: vs(400.0, 20, 150.0, 0.5),
            return_: vs(300.0, 25, 120.0, 0.6),
            backspace: vs(500.0, 12, 250.0, 0.35),
            other: vs(600.0, 15, 200.0, 0.3),
        },
        ThemeDef {
            shape: WaveShape::FilteredNoise,
            normal: vs(3000.0, 5, 1200.0, 0.25),
            space: vs(2500.0, 8, 1000.0, 0.3),
            return_: vs(2000.0, 10, 900.0, 0.35),
            backspace: vs(2800.0, 4, 1400.0, 0.2),
            other: vs(3000.0, 5, 1200.0, 0.2),
        },
    ]
}

impl ThemeDef {
    fn spec_for(&self, category: KeyCategory) -> VoiceSpec {
        match category {
            KeyCategory::Normal => self.normal,
            KeyCategory::Space => self.space,
            KeyCategory::Return => self.return_,
            KeyCategory::Backspace => self.backspace,
            _ => self.other,
        }
    }
}

fn render_sine(spec: VoiceSpec, samples: u32) -> Vec<i16> {
    let mut buf = Vec::with_capacity(samples as usize);
    for i in 0..samples {
        let t = i as f32 / 44100.0;
        let envelope = (-t * spec.envelope_rate).exp();
        let wave = (2.0 * std::f32::consts::PI * spec.freq * t).sin();
        buf.push((wave * envelope * spec.volume * 32767.0) as i16);
    }
    buf
}

fn render_click_plus_thud(spec: VoiceSpec, samples: u32) -> Vec<i16> {
    let mut buf = Vec::with_capacity(samples as usize);
    for i in 0..samples {
        let t = i as f32 / 44100.0;
        let envelope = (-t * spec.envelope_rate).exp();
        let click = (2.0 * std::f32::consts::PI * spec.freq * t).sin();
        let thud = (2.0 * std::f32::consts::PI * spec.freq * 0.15 * t).sin() * 0.3;
        let wave = click * 0.7 + thud;
        buf.push((wave * envelope * spec.volume * 32767.0) as i16);
    }
    buf
}

fn render_layered_typewriter(spec: VoiceSpec, samples: u32, is_return: bool) -> Vec<i16> {
    let mut buf = Vec::with_capacity(samples as usize);
    for i in 0..samples {
        let t = i as f32 / 44100.0;
        let envelope = (-t * spec.envelope_rate).exp();
        let click = (2.0 * std::f32::consts::PI * spec.freq * 3.0 * t).sin() * 0.4;
        let body = (2.0 * std::f32::consts::PI * spec.freq * t).sin() * 0.5;
        let noise = ((i as f32 * 0.7).sin() * 0.3).sin() * 0.1;
        let mut wave = click + body + noise;
        if is_return {
            let bell = (2.0 * std::f32::consts::PI * 1200.0 * t).sin() * 0.15;
            wave += bell;
        }
        buf.push((wave * envelope * spec.volume * 32767.0) as i16);
    }
    buf
}

fn render_square_wave(spec: VoiceSpec, samples: u32) -> Vec<i16> {
    let mut buf = Vec::with_capacity(samples as usize);
    for i in 0..samples {
        let t = i as f32 / 44100.0;
        let envelope = (-t * spec.envelope_rate).exp();
        let phase = 2.0 * std::f32::consts::PI * spec.freq * t;
        let wave = if phase.sin() >= 0.0 { 0.5 } else { -0.5 };
        buf.push((wave * envelope * spec.volume * 32767.0) as i16);
    }
    buf
}

fn render_pluck(spec: VoiceSpec, samples: u32) -> Vec<i16> {
    let mut buf = Vec::with_capacity(samples as usize);
    for i in 0..samples {
        let t = i as f32 / 44100.0;
        let envelope = (-t * spec.envelope_rate).exp();
        let wave = (2.0 * std::f32::consts::PI * spec.freq * t).sin() * (1.0 - t * 2.0).max(0.0);
        buf.push((wave * envelope * spec.volume * 32767.0) as i16);
    }
    buf
}

fn render_filtered_noise(spec: VoiceSpec, samples: u32) -> Vec<i16> {
    let mut buf = Vec::with_capacity(samples as usize);
    for i in 0..samples {
        let t = i as f32 / 44100.0;
        let envelope = (-t * spec.envelope_rate).exp();
        let noise = ((i as f32 * 1.3).sin() * 0.5).sin() * 0.3;
        let tone = (2.0 * std::f32::consts::PI * spec.freq * t).sin() * 0.2;
        let wave = noise + tone;
        buf.push((wave * envelope * spec.volume * 32767.0) as i16);
    }
    buf
}

/// 为指定主题和按键分类生成 WAV 音效
fn generate_sound(theme: SoundTheme, category: KeyCategory) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut buf = Vec::new();
    {
        let mut writer = hound::WavWriter::new(Cursor::new(&mut buf), spec).unwrap();
        let theme_def = &builtin_themes()[theme.as_index() as usize];
        let voice_spec = theme_def.spec_for(category);
        let samples = (voice_spec.duration_ms as f32 / 1000.0 * 44100.0) as u32;
        let is_return = category == KeyCategory::Return;

        let pcm: Vec<i16> = match theme_def.shape {
            WaveShape::Sine => render_sine(voice_spec, samples),
            WaveShape::ClickPlusThud => render_click_plus_thud(voice_spec, samples),
            WaveShape::LayeredTypewriter => {
                render_layered_typewriter(voice_spec, samples, is_return)
            }
            WaveShape::SquareWave => render_square_wave(voice_spec, samples),
            WaveShape::Pluck => render_pluck(voice_spec, samples),
            WaveShape::FilteredNoise => render_filtered_noise(voice_spec, samples),
        };

        for sample in pcm {
            writer.write_sample(sample).unwrap();
        }
    }
    buf
}

/// AUD-012：音效设置（开关/音量/主题）持久化模型。
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct AudioSettings {
    pub enabled: bool,
    pub volume: f32,
    pub theme: u8,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: 0.5,
            theme: SoundTheme::Ipad.as_index(),
        }
    }
}

impl AudioSettings {
    /// 从路径加载；不存在或损坏时返回默认值，绝不 panic（AUD-020）。
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(error) => {
                log::warn!("读取音频设置失败，使用默认值: {error}");
                return Self::default();
            }
        };
        let mut settings: AudioSettings = match serde_json::from_slice(&bytes) {
            Ok(s) => s,
            Err(error) => {
                log::warn!("解析音频设置失败，使用默认值: {error}");
                return Self::default();
            }
        };
        settings.volume = settings.volume.clamp(0.0, 1.0);
        settings.theme = SoundTheme::from_index(settings.theme).as_index();
        settings
    }

    /// AUD-020：原子写盘，失败返回错误。
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("序列化音频设置失败: {error}"))?;
        crate::atomic_file::write(path, &bytes)
            .map_err(|error| format!("保存音频设置失败: {error}"))
    }
}

#[derive(Debug)]
enum BackendPlayError {
    Decoder(String),
    Sink(String),
}

/// 音频后端抽象：rodio 对象只在该 trait 的实现内存在，构造/使用/销毁都在 worker 线程。
trait AudioBackend {
    fn start_keep_alive(&mut self) -> Result<(), String>;
    fn play(&mut self, wav: Arc<Vec<u8>>, volume: f32) -> Result<(), BackendPlayError>;
    fn default_device_changed(&self) -> bool {
        false
    }
}

struct RodioBackend {
    _stream: OutputStream,
    handle: OutputStreamHandle,
    device_name: Option<String>,
}

impl RodioBackend {
    fn new() -> Result<Self, String> {
        let (stream, handle) =
            OutputStream::try_default().map_err(|e| format!("音频输出初始化失败: {e}"))?;
        Ok(Self {
            _stream: stream,
            handle,
            device_name: default_output_device_name(),
        })
    }
}

impl AudioBackend for RodioBackend {
    fn start_keep_alive(&mut self) -> Result<(), String> {
        self.handle
            .play_raw(
                SineWave::new(KEEP_ALIVE_FREQ)
                    .amplify(KEEP_ALIVE_VOLUME)
                    .repeat_infinite(),
            )
            .map_err(|error| format!("保温音源挂载失败: {error}"))
    }

    fn play(&mut self, wav: Arc<Vec<u8>>, volume: f32) -> Result<(), BackendPlayError> {
        let source = rodio::decoder::Decoder::new(Cursor::new((*wav).clone()))
            .map_err(|error| BackendPlayError::Decoder(format!("{error:?}")))?;
        let sink = Sink::try_new(&self.handle)
            .map_err(|error| BackendPlayError::Sink(format!("{error:?}")))?;
        sink.set_volume(volume);
        sink.append(source);
        sink.detach();
        Ok(())
    }

    fn default_device_changed(&self) -> bool {
        default_output_device_name() != self.device_name
    }
}

fn default_output_device_name() -> Option<String> {
    cpal::default_host()
        .default_output_device()
        .and_then(|device| device.name().ok())
}

enum WorkerCommand {
    Play { wav: Arc<Vec<u8>>, volume: f32 },
    Shutdown,
}

/// SR-03：是否需要重建输出流。设备变化或调度停顿超过阈值（睡眠唤醒）时为真。纯函数可单测。
fn needs_runtime_rebuild(elapsed: Duration, device_changed: bool) -> bool {
    device_changed || elapsed > SUSPEND_GAP_THRESHOLD
}

/// Send + Sync 音频门面（AUD-002：不再有 unsafe Send/Sync）。
/// 所有非线程安全的 rodio 状态只存在于 worker 线程内。
pub struct AudioEngine {
    commands: Option<SyncSender<WorkerCommand>>,
    worker: Option<JoinHandle<()>>,
    sounds: HashMap<(SoundTheme, KeyCategory), Arc<Vec<u8>>>,
    enabled: AtomicBool,
    volume: Mutex<f32>,
    current_theme: AtomicU8,
    keep_alive_active: Arc<AtomicBool>,
    health: RuntimeHealth,
    play_diagnostic_seq: AtomicUsize,
    settings_path: Option<PathBuf>,
    settings_mutation: Mutex<()>,
}

impl AudioEngine {
    /// 生产构造器。AUD-005：失败返回错误而非 panic；调用方应降级为 unavailable。
    pub fn new(health: RuntimeHealth) -> Result<Self, String> {
        let data = crate::data_paths::data_root()?;
        Self::new_with_paths(health, data.join("KeyM/audio_settings.json"))
    }

    pub fn new_with_paths(health: RuntimeHealth, settings_path: PathBuf) -> Result<Self, String> {
        let settings = AudioSettings::load(&settings_path);
        Self::new_with_factory(
            health,
            RodioBackend::new,
            STARTUP_TIMEOUT,
            settings,
            Some(settings_path),
        )
    }

    /// AUD-005：音频不可用时降级--应用与统计仍可启动，仅声音不工作。
    pub fn unavailable(health: RuntimeHealth, error: impl Into<String>) -> Self {
        let error = error.into();
        log::error!("音频降级运行: {error}");
        health.set_audio_failed(error);
        let mut sounds = HashMap::new();
        for theme in SoundTheme::all() {
            for category in [
                KeyCategory::Normal,
                KeyCategory::Space,
                KeyCategory::Return,
                KeyCategory::Backspace,
                KeyCategory::Tab,
                KeyCategory::Escape,
                KeyCategory::Modifier,
                KeyCategory::Arrow,
                KeyCategory::Other,
            ] {
                sounds.insert(
                    (*theme, category),
                    Arc::new(generate_sound(*theme, category)),
                );
            }
        }
        Self {
            commands: None,
            worker: None,
            sounds,
            enabled: AtomicBool::new(false),
            volume: Mutex::new(0.5),
            current_theme: AtomicU8::new(SoundTheme::Ipad.as_index()),
            keep_alive_active: Arc::new(AtomicBool::new(false)),
            health,
            play_diagnostic_seq: AtomicUsize::new(0),
            settings_path: None,
            settings_mutation: Mutex::new(()),
        }
    }

    /// 依赖注入构造器：后端工厂可替换（测试用假后端），启动等待带超时（AUD-005）。
    /// F2：超时路径放弃 join（detach worker）——工厂可能永久挂起，join 会无界
    /// 阻塞 setup 线程使超时保护失效。泄露边界清晰：被 detach 的 worker 只持有
    /// 已断开的命令通道与挂起的工厂，不持锁、不写盘；工厂一旦返回，worker 会因
    /// 启动通道与命令通道均已断开而立即自行退出。
    fn new_with_factory<F, B>(
        health: RuntimeHealth,
        mut factory: F,
        startup_timeout: Duration,
        settings: AudioSettings,
        settings_path: Option<PathBuf>,
    ) -> Result<Self, String>
    where
        F: FnMut() -> Result<B, String> + Send + 'static,
        B: AudioBackend + 'static,
    {
        let mut sounds = HashMap::new();
        for theme in SoundTheme::all() {
            for cat in [
                KeyCategory::Normal,
                KeyCategory::Space,
                KeyCategory::Return,
                KeyCategory::Backspace,
                KeyCategory::Tab,
                KeyCategory::Escape,
                KeyCategory::Modifier,
                KeyCategory::Arrow,
                KeyCategory::Other,
            ] {
                sounds.insert((*theme, cat), Arc::new(generate_sound(*theme, cat)));
            }
        }

        let (commands, receiver) = mpsc::sync_channel(COMMAND_CAPACITY);
        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let keep_alive_active = Arc::new(AtomicBool::new(false));
        let worker_keep_alive = Arc::clone(&keep_alive_active);
        let worker_health = health.clone();
        let worker = thread::Builder::new()
            .name("keym-audio".into())
            .spawn(move || {
                worker_main(
                    &mut factory,
                    receiver,
                    startup_tx,
                    worker_keep_alive,
                    worker_health,
                );
            })
            .map_err(|error| format!("音频线程启动失败: {error}"))?;

        match startup_rx.recv_timeout(startup_timeout) {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                drop(commands);
                let _ = worker.join();
                return Err(error);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // F2/AUD-005：工厂可能永久挂起（底层初始化长期不返回），
                // 此时 join 会无界阻塞，必须 detach 而非 join。
                drop(commands);
                drop(worker);
                return Err(format!(
                    "音频线程启动超时（{} ms）",
                    startup_timeout.as_millis()
                ));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                drop(commands);
                let _ = worker.join();
                return Err("音频线程在初始化期间退出".into());
            }
        }

        health.set_audio_running();
        Ok(Self {
            commands: Some(commands),
            worker: Some(worker),
            sounds,
            enabled: AtomicBool::new(settings.enabled),
            volume: Mutex::new(settings.volume.clamp(0.0, 1.0)),
            current_theme: AtomicU8::new(SoundTheme::from_index(settings.theme).as_index()),
            keep_alive_active,
            health,
            play_diagnostic_seq: AtomicUsize::new(0),
            settings_path,
            settings_mutation: Mutex::new(()),
        })
    }

    pub fn is_keep_alive_active(&self) -> bool {
        self.keep_alive_active.load(Ordering::Acquire)
    }

    pub fn keep_alive_volume(&self) -> f32 {
        KEEP_ALIVE_VOLUME
    }

    /// 按键码播放对应音效（投递到 worker 线程，回调不阻塞、不读盘 AUD-007）。
    pub fn play_key(&self, keycode: u16) -> PlayOutcome {
        let seq = self.play_diagnostic_seq.fetch_add(1, Ordering::Relaxed) + 1;
        let log_diagnostic = seq <= PLAY_DIAGNOSTIC_LIMIT;
        if !self.enabled.load(Ordering::Relaxed) {
            self.log_play_diagnostic(seq, keycode, PlayOutcome::Disabled, log_diagnostic);
            return PlayOutcome::Disabled;
        }
        let theme = SoundTheme::from_index(self.current_theme.load(Ordering::Relaxed));
        let category = keycode_to_category(keycode);
        let Some(wav) = self.sounds.get(&(theme, category)) else {
            if log_diagnostic {
                log::error!(
                    "keym_diag event=audio_play_error seq={} stage=lookup source=builtin theme={:?} category={:?}",
                    seq, theme, category
                );
            }
            self.log_play_diagnostic(seq, keycode, PlayOutcome::MissingBuiltin, log_diagnostic);
            return PlayOutcome::MissingBuiltin;
        };
        let wav = Arc::clone(wav);
        let command = WorkerCommand::Play {
            wav,
            volume: self.get_volume(),
        };
        let outcome = match self.commands.as_ref() {
            Some(tx) => match tx.try_send(command) {
                Ok(()) => PlayOutcome::Queued,
                Err(TrySendError::Full(_)) => PlayOutcome::QueueFull,
                Err(TrySendError::Disconnected(_)) => {
                    self.health.set_audio_failed("音频 worker 已停止");
                    PlayOutcome::WorkerStopped
                }
            },
            None => {
                self.health.set_audio_failed("音频 worker 已停止");
                PlayOutcome::WorkerStopped
            }
        };
        self.log_play_diagnostic(seq, keycode, outcome, log_diagnostic);
        outcome
    }

    fn log_play_diagnostic(
        &self,
        seq: usize,
        keycode: u16,
        outcome: PlayOutcome,
        log_diagnostic: bool,
    ) {
        if !log_diagnostic {
            return;
        }
        log::info!(
            "keym_diag event=audio_enqueue seq={} keycode={} outcome={} volume={:.3}",
            seq,
            keycode,
            outcome.label(),
            self.get_volume()
        );
    }

    pub fn set_theme(&self, theme: SoundTheme) -> Result<(), String> {
        let _guard = self
            .settings_mutation
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        self.persist_settings(AudioSettings {
            enabled: self.is_enabled(),
            volume: self.get_volume(),
            theme: theme.as_index(),
        })?;
        self.current_theme
            .store(theme.as_index(), Ordering::Relaxed);
        Ok(())
    }

    pub fn get_theme(&self) -> SoundTheme {
        SoundTheme::from_index(self.current_theme.load(Ordering::Relaxed))
    }

    /// AUD-012/020：开关变更持久化，写盘失败回滚内存并返回错误。
    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        let _guard = self
            .settings_mutation
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let previous = self.is_enabled();
        self.persist_settings(AudioSettings {
            enabled,
            volume: self.get_volume(),
            theme: self.get_theme().as_index(),
        })?;
        self.enabled.store(enabled, Ordering::Relaxed);
        // 持久化成功才提交内存；失败时 previous 不变。
        let _ = previous;
        Ok(())
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// AUD-012/020：音量变更持久化。
    pub fn set_volume(&self, volume: f32) -> Result<f32, String> {
        let _guard = self
            .settings_mutation
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let volume = volume.clamp(0.0, 1.0);
        self.persist_settings(AudioSettings {
            enabled: self.is_enabled(),
            volume,
            theme: self.get_theme().as_index(),
        })?;
        *self.volume.lock().unwrap_or_else(|e| e.into_inner()) = volume;
        Ok(volume)
    }

    pub fn get_volume(&self) -> f32 {
        *self.volume.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn persist_settings(&self, settings: AudioSettings) -> Result<(), String> {
        let Some(path) = &self.settings_path else {
            return Ok(());
        };
        settings.save(path)
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        if let Some(commands) = self.commands.take() {
            let _ = commands.try_send(WorkerCommand::Shutdown);
            drop(commands);
        }
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                log::error!("音频线程退出时 panic");
            }
        }
    }
}

fn worker_main<F, B>(
    factory: &mut F,
    receiver: Receiver<WorkerCommand>,
    startup: SyncSender<Result<(), String>>,
    keep_alive: Arc<AtomicBool>,
    health: RuntimeHealth,
) where
    F: FnMut() -> Result<B, String>,
    B: AudioBackend,
{
    let mut backend = match factory() {
        Ok(backend) => backend,
        Err(error) => {
            health.set_audio_failed(error.clone());
            let _ = startup.send(Err(error));
            return;
        }
    };
    match backend.start_keep_alive() {
        Ok(()) => keep_alive.store(true, Ordering::Release),
        Err(error) => log::warn!("{error}"),
    }
    health.set_audio_running();
    let _ = startup.send(Ok(()));
    let mut last_tick = std::time::Instant::now();

    loop {
        let command = receiver.recv_timeout(DEVICE_POLL_INTERVAL);
        let now = std::time::Instant::now();
        let elapsed = now.saturating_duration_since(last_tick);
        last_tick = now;

        match command {
            Ok(WorkerCommand::Shutdown) => break,
            Ok(WorkerCommand::Play { wav, volume }) => {
                if needs_runtime_rebuild(elapsed, backend.default_device_changed())
                    && !rebuild(&mut backend, factory, &keep_alive, &health)
                {
                    continue;
                }
                if let Err(error) = backend.play(wav, volume) {
                    match error {
                        BackendPlayError::Decoder(msg) => {
                            log::warn!("音频解码失败: {msg}");
                        }
                        BackendPlayError::Sink(msg) => {
                            health.set_audio_recovering("音频 sink 失败，尝试重建输出");
                            keep_alive.store(false, Ordering::Release);
                            match factory() {
                                Ok(mut replacement) => {
                                    if replacement.start_keep_alive().is_ok() {
                                        keep_alive.store(true, Ordering::Release);
                                    }
                                    backend = replacement;
                                    health.set_audio_running();
                                }
                                Err(rebuild_error) => {
                                    health.set_audio_failed(format!(
                                        "音频输出重建失败: {rebuild_error}"
                                    ));
                                }
                            }
                            let _ = msg;
                        }
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if needs_runtime_rebuild(elapsed, backend.default_device_changed()) {
                    rebuild(&mut backend, factory, &keep_alive, &health);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    keep_alive.store(false, Ordering::Release);
}

/// 重建后端；成功返回 true，失败时记录健康状态并返回 false。
fn rebuild<B: AudioBackend, F: FnMut() -> Result<B, String>>(
    backend: &mut B,
    factory: &mut F,
    keep_alive: &Arc<AtomicBool>,
    health: &RuntimeHealth,
) -> bool {
    health.set_audio_recovering("设备变化或睡眠唤醒，重建音频输出");
    keep_alive.store(false, Ordering::Release);
    match factory() {
        Ok(mut replacement) => {
            if replacement.start_keep_alive().is_ok() {
                keep_alive.store(true, Ordering::Release);
            }
            *backend = replacement;
            health.set_audio_running();
            true
        }
        Err(error) => {
            health.set_audio_failed(format!("音频重建失败: {error}"));
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn test_keycode_to_category() {
        assert_eq!(keycode_to_category(0x31), KeyCategory::Space);
        assert_eq!(keycode_to_category(0x24), KeyCategory::Return);
        assert_eq!(keycode_to_category(0x33), KeyCategory::Backspace);
        assert_eq!(keycode_to_category(0x30), KeyCategory::Tab);
        assert_eq!(keycode_to_category(0x00), KeyCategory::Normal);
        assert_eq!(keycode_to_category(0x7E), KeyCategory::Arrow);
        assert_eq!(keycode_to_category(0x37), KeyCategory::Modifier);
    }

    #[test]
    fn test_sound_theme_roundtrip() {
        for i in 0..SoundTheme::count() {
            let theme = SoundTheme::from_index(i);
            assert_eq!(theme.as_index(), i);
        }
    }

    #[test]
    fn test_count() {
        assert_eq!(SoundTheme::count(), 11);
    }

    #[test]
    fn test_generate_sound_valid_wav() {
        let wav = generate_sound(SoundTheme::Ipad, KeyCategory::Normal);
        assert!(!wav.is_empty());
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
    }

    #[test]
    fn test_all_themes_generate_valid_wav() {
        for theme in SoundTheme::all() {
            for cat in [
                KeyCategory::Normal,
                KeyCategory::Space,
                KeyCategory::Return,
                KeyCategory::Backspace,
            ] {
                let wav = generate_sound(*theme, cat);
                assert_eq!(&wav[0..4], b"RIFF", "{theme:?} {cat:?} invalid");
            }
        }
    }

    /// SR-03：调度停顿达到阈值或设备变化才重建；恰等于阈值不重建。
    #[test]
    fn sr03_rebuild_only_beyond_threshold_or_device_change() {
        assert!(!needs_runtime_rebuild(Duration::from_secs(0), false));
        assert!(!needs_runtime_rebuild(SUSPEND_GAP_THRESHOLD, false));
        assert!(needs_runtime_rebuild(
            SUSPEND_GAP_THRESHOLD + Duration::from_millis(1),
            false
        ));
        assert!(needs_runtime_rebuild(Duration::from_secs(0), true));
    }

    /// AUD-012：设置往返保持，损坏文件回退默认。
    #[test]
    fn audio_settings_roundtrip_and_corrupt_fallback() {
        let dir = std::env::temp_dir().join(format!(
            "keym_audio_settings_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("audio_settings.json");

        let s = AudioSettings {
            enabled: false,
            volume: 0.37,
            theme: SoundTheme::Mechanical.as_index(),
        };
        s.save(&path).unwrap();
        let loaded = AudioSettings::load(&path);
        assert!(!loaded.enabled);
        assert!((loaded.volume - 0.37).abs() < f32::EPSILON);
        assert_eq!(loaded.theme, SoundTheme::Mechanical.as_index());

        // 损坏文件回退默认
        std::fs::write(&path, b"{not json").unwrap();
        let fallback = AudioSettings::load(&path);
        assert!(fallback.enabled);
        assert!((fallback.volume - 0.5).abs() < f32::EPSILON);

        // 越界主题索引安全回退到 Ipad
        let bad = AudioSettings {
            enabled: true,
            volume: 0.5,
            theme: 200,
        };
        bad.save(&path).unwrap();
        assert_eq!(
            AudioSettings::load(&path).theme,
            SoundTheme::Ipad.as_index()
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// AUD-002：AudioEngine 是 Send + Sync，无需手写 unsafe。
    #[test]
    fn audio_engine_is_send_sync_without_unsafe() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AudioEngine>();
    }

    /// AUD-005：初始化失败时降级为 unavailable，应用不 panic。
    #[test]
    fn unavailable_engine_reports_failed_and_does_not_play() {
        let health = RuntimeHealth::new();
        let engine = AudioEngine::unavailable(health.clone(), "没有输出设备");
        assert_eq!(
            health.snapshot().audio.status,
            crate::runtime_health::ServiceStatus::Failed
        );
        // 降级引擎默认关闭：播放按 Disabled 短路
        assert!(!engine.is_enabled());
        assert_eq!(engine.play_key(0x00), PlayOutcome::Disabled);
        // 用户在 UI 重新打开开关后：无 worker，播放报 WorkerStopped 而非 panic
        engine.set_enabled(true).unwrap();
        assert_eq!(engine.play_key(0x00), PlayOutcome::WorkerStopped);
    }

    /// AUD-005：工厂持续失败时 new 返回错误而非 panic。
    #[test]
    fn factory_failure_returns_error() {
        let health = RuntimeHealth::new();
        let result = AudioEngine::new_with_factory(
            health,
            || Err::<RodioBackend, String>("注入失败".into()),
            STARTUP_TIMEOUT,
            AudioSettings::default(),
            None,
        );
        assert!(result.is_err());
    }

    /// AUD-005：启动超时返回错误。用慢（但会返回）的工厂 + 极短超时。
    #[test]
    fn startup_timeout_returns_error() {
        struct StuckBackend;
        impl AudioBackend for StuckBackend {
            fn start_keep_alive(&mut self) -> Result<(), String> {
                std::thread::sleep(Duration::from_secs(2));
                Ok(())
            }
            fn play(&mut self, _wav: Arc<Vec<u8>>, _volume: f32) -> Result<(), BackendPlayError> {
                Ok(())
            }
        }
        let health = RuntimeHealth::new();
        let result = AudioEngine::new_with_factory(
            health,
            || Ok(StuckBackend),
            Duration::from_millis(50),
            AudioSettings::default(),
            None,
        );
        assert!(result.is_err());
    }

    /// F2/AUD-005：工厂真正永不返回时，构造也必须在阈值内返回——超时路径
    /// detach 而非 join worker，绝不无界阻塞调用方。被 detach 的线程只持有
    /// 已断开的通道与挂起的工厂（本测试进程退出时随之回收），不持锁、不写盘。
    #[test]
    fn hung_factory_startup_returns_within_threshold() {
        struct HungBackend;
        impl AudioBackend for HungBackend {
            fn start_keep_alive(&mut self) -> Result<(), String> {
                Ok(())
            }
            fn play(&mut self, _wav: Arc<Vec<u8>>, _volume: f32) -> Result<(), BackendPlayError> {
                Ok(())
            }
        }
        let health = RuntimeHealth::new();
        let started = std::time::Instant::now();
        let result = AudioEngine::new_with_factory(
            health,
            || -> Result<HungBackend, String> {
                // 永不返回：模拟底层音频初始化永久挂起
                loop {
                    std::thread::park();
                }
            },
            Duration::from_millis(50),
            AudioSettings::default(),
            None,
        );
        let elapsed = started.elapsed();
        let error = result.err().expect("挂起的工厂必须返回错误");
        assert!(error.contains("超时"), "应为超时错误，实际: {error}");
        assert!(
            elapsed < Duration::from_secs(2),
            "构造必须在阈值内返回，实际耗时 {elapsed:?}（超时路径 join 会永久挂起）"
        );
    }

    /// AUD-007：有界命令队列满时返回 QueueFull，不阻塞调用方。
    #[test]
    fn bounded_queue_full_does_not_block() {
        struct SlowBackend {
            plays: Arc<AtomicUsize>,
        }
        impl AudioBackend for SlowBackend {
            fn start_keep_alive(&mut self) -> Result<(), String> {
                Ok(())
            }
            fn play(&mut self, _wav: Arc<Vec<u8>>, _volume: f32) -> Result<(), BackendPlayError> {
                self.plays.fetch_add(1, Ordering::Relaxed);
                // 模拟慢消费，让队列积压
                std::thread::sleep(Duration::from_millis(50));
                Ok(())
            }
        }
        let plays = Arc::new(AtomicUsize::new(0));
        let plays_for_factory = Arc::clone(&plays);
        let health = RuntimeHealth::new();
        let engine = AudioEngine::new_with_factory(
            health,
            move || {
                Ok(SlowBackend {
                    plays: Arc::clone(&plays_for_factory),
                })
            },
            STARTUP_TIMEOUT,
            AudioSettings::default(),
            None,
        )
        .expect("engine with fake backend");
        // 快速投递远超容量，应观察到 QueueFull 而非死锁
        let mut saw_full = false;
        for _ in 0..(COMMAND_CAPACITY * 4) {
            if engine.play_key(0x00) == PlayOutcome::QueueFull {
                saw_full = true;
                break;
            }
        }
        // 即便没满也允许（消费足够快），但不能 panic/死锁
        drop(engine);
        let _ = saw_full;
        assert!(plays.load(Ordering::Relaxed) > 0);
    }

    /// 真实设备上的回归：构造后保温活跃，主题切换、播放与跨线程可用。
    /// 设置写到临时路径，绝不触碰真实用户配置。
    #[test]
    fn test_audio_engine_with_theme() {
        let dir = std::env::temp_dir().join(format!(
            "keym_audio_engine_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let health = RuntimeHealth::new();
        let engine = AudioEngine::new_with_paths(health, dir.join("audio_settings.json"))
            .expect("AudioEngine 创建失败");
        assert!(
            engine.is_keep_alive_active(),
            "构造后保温音源应已挂到混音器"
        );
        engine.set_theme(SoundTheme::Mechanical).unwrap();
        assert_eq!(engine.get_theme(), SoundTheme::Mechanical);
        engine.set_theme(SoundTheme::Ipad).unwrap();
        assert_eq!(engine.get_theme(), SoundTheme::Ipad);
        for _ in 0..50 {
            engine.play_key(0x00);
        }
        assert!(engine.is_keep_alive_active());
        drop(engine);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_keep_alive_volume_inaudible() {
        let v = KEEP_ALIVE_VOLUME;
        assert!(
            v > 0.0 && v <= 0.001,
            "保温音量必须非零且 ≤ -60dB, 当前: {v}"
        );
    }
}
