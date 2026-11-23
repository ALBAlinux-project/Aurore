use chrono::{Datelike, Local, Timelike};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

const CSS_PATH: &str = "/tmp/aurore_active.css";
const CONFIG_INTERVAL: Duration = Duration::from_secs(2);
const WEATHER_INTERVAL: Duration = Duration::from_secs(900);
const WEATHER_RETRY: Duration = Duration::from_secs(60);
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, PartialEq)]
enum Weather {
    Clear,
    Cloudy,
    Rain,
    Snow,
}

#[derive(Clone, Deserialize)]
#[serde(default)]
struct Config {
    lat: f64,
    lon: f64,
    base_hue: f64,
    saturation_max: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            lat: 35.6895,
            lon: 139.6917,
            base_hue: 0.38,
            saturation_max: 0.55,
        }
    }
}

#[derive(Deserialize)]
struct OpenMeteoRes {
    current_weather: CurrentWeather,
}

#[derive(Deserialize)]
struct CurrentWeather {
    weathercode: i32,
}

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".config/aurore/aurore.toml")
}

// 設定ファイルがなければ既定値を使う。書式が壊れている場合も既定値に戻す。
fn load_config() -> Config {
    fs::read_to_string(config_path())
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

fn fetch_weather(lat: f64, lon: f64) -> Option<Weather> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current_weather=true",
        lat, lon
    );
    let data: OpenMeteoRes = ureq::get(&url)
        .timeout(HTTP_TIMEOUT)
        .call()
        .ok()?
        .into_json()
        .ok()?;

    Some(match data.current_weather.weathercode {
        0 | 1 => Weather::Clear,
        2 | 3 | 45 | 48 => Weather::Cloudy,
        71 | 73 | 75 | 77 | 85 | 86 => Weather::Snow,
        _ => Weather::Rain,
    })
}

fn current_hex(cfg: &Config, weather: Weather) -> String {
    let now = Local::now();
    let sec = (now.hour() * 3600 + now.minute() * 60 + now.second()) as f64;
    let daylight = 1.0 - ((sec / 86400.0) - 0.5).abs() * 2.0;

    let season_shift = match now.month() {
        3..=5 => -0.04,
        6..=8 => 0.0,
        9..=11 => -0.06,
        _ => 0.05,
    };

    let (sat, val_mult) = match weather {
        Weather::Clear => (0.55, 1.00),
        Weather::Cloudy => (0.30, 0.85),
        Weather::Rain => (0.35, 0.72),
        Weather::Snow => (0.20, 1.05),
    };

    // 設定の彩度上限を超えないように抑える
    let sat = sat.min(cfg.saturation_max);
    let hue = (cfg.base_hue + season_shift).rem_euclid(1.0);
    let val = ((0.14 + daylight * 0.38) * val_mult).clamp(0.08, 0.90);

    let (r, g, b) = hsv_to_rgb(hue, sat, val);
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match (i as i32) % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

// 一時ファイルに書いてから rename するので、読み手が途中の内容を見ない。
fn write_css_atomic(path: &Path, hex: &str) {
    let content = format!("@define-color aurore_primary {};\n", hex);
    let tmp = path.with_extension("css.tmp");
    if fs::write(&tmp, content).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

fn reload_waybar() {
    let _ = Command::new("pkill").args(["-SIGUSR2", "waybar"]).status();
}

fn main() {
    let config = Arc::new(Mutex::new(load_config()));
    let weather = Arc::new(Mutex::new(Weather::Clear));

    // 設定ファイルを定期的に読み直す
    let config_writer = Arc::clone(&config);
    thread::spawn(move || loop {
        let fresh = load_config();
        if let Ok(mut cfg) = config_writer.lock() {
            *cfg = fresh;
        }
        thread::sleep(CONFIG_INTERVAL);
    });

    // 座標が変わったら即座に取り直し、通常は15分ごとに更新する
    let config_reader = Arc::clone(&config);
    let weather_writer = Arc::clone(&weather);
    thread::spawn(move || {
        let mut next_attempt = Instant::now();
        let mut last_coords: Option<(f64, f64)> = None;
        loop {
            let (lat, lon) = match config_reader.lock() {
                Ok(cfg) => (cfg.lat, cfg.lon),
                Err(_) => (35.6895, 139.6917),
            };

            if last_coords != Some((lat, lon)) {
                next_attempt = Instant::now();
            }

            if Instant::now() >= next_attempt {
                match fetch_weather(lat, lon) {
                    Some(w) => {
                        if let Ok(mut current) = weather_writer.lock() {
                            *current = w;
                        }
                        next_attempt = Instant::now() + WEATHER_INTERVAL;
                    }
                    None => {
                        next_attempt = Instant::now() + WEATHER_RETRY;
                    }
                }
                last_coords = Some((lat, lon));
            }

            thread::sleep(CONFIG_INTERVAL);
        }
    });

    let css_path = Path::new(CSS_PATH);
    let mut last_hex = String::new();

    loop {
        let cfg = config.lock().map(|c| c.clone()).unwrap_or_default();
        let w = weather.lock().map(|w| *w).unwrap_or(Weather::Clear);
        let hex = current_hex(&cfg, w);

        if hex != last_hex {
            write_css_atomic(css_path, &hex);
            reload_waybar();
            last_hex = hex;
        }

        thread::sleep(Duration::from_secs(1));
    }
}
