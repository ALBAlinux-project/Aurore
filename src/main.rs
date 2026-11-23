use chrono::{Datelike, Local, Timelike};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

const LAT: f64 = 35.6895;
const LON: f64 = 139.6917;
const BASE_HUE: f64 = 0.38;
const WEATHER_INTERVAL: Duration = Duration::from_secs(900);
const COLOR_INTERVAL: Duration = Duration::from_secs(1);
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, PartialEq)]
enum Weather {
    Clear,
    Cloudy,
    Rain,
    Snow,
}

#[derive(Deserialize)]
struct OpenMeteoRes {
    current_weather: CurrentWeather,
}

#[derive(Deserialize)]
struct CurrentWeather {
    weathercode: i32,
}

struct AuroreEngine {
    base_hue: f64,
    weather: Weather,
}

impl AuroreEngine {
    fn new(base_hue: f64) -> Self {
        Self {
            base_hue,
            weather: Weather::Clear,
        }
    }

    fn current_hex(&self) -> String {
        let now = Local::now();
        let sec = (now.hour() * 3600 + now.minute() * 60 + now.second()) as f64;
        let daylight = 1.0 - ((sec / 86400.0) - 0.5).abs() * 2.0;

        let season_shift = match now.month() {
            3..=5 => -0.04,
            6..=8 => 0.0,
            9..=11 => -0.06,
            _ => 0.05,
        };

        let (sat, val_mult) = match self.weather {
            Weather::Clear => (0.55, 1.00),
            Weather::Cloudy => (0.30, 0.85),
            Weather::Rain => (0.35, 0.72),
            Weather::Snow => (0.20, 1.05),
        };

        let hue = (self.base_hue + season_shift).rem_euclid(1.0);
        let val = ((0.14 + daylight * 0.38) * val_mult).clamp(0.08, 0.90);

        let (r, g, b) = hsv_to_rgb(hue, sat, val);
        format!("#{:02x}{:02x}{:02x}", r, g, b)
    }
}

/// 通信は Mutex の外で行う。失敗時は None を返す。
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

fn get_css_output_path() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        let mut p = PathBuf::from(home);
        p.push(".cache/aurore/color.css");
        p
    } else {
        PathBuf::from("/tmp/aurore_color.css")
    }
}

/// 一時ファイルに書いてから rename するので、読み手が途中の内容を見ない。
fn write_css_atomic(path: &Path, hex: &str) {
    let content = format!("@define-color aurore_primary {};\n", hex);
    let tmp = path.with_extension("css.tmp");
    if fs::write(&tmp, content).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

fn main() {
    let engine = Arc::new(Mutex::new(AuroreEngine::new(BASE_HUE)));

    let weather_engine = Arc::clone(&engine);
    thread::spawn(move || loop {
        if let Some(w) = fetch_weather(LAT, LON) {
            if let Ok(mut eng) = weather_engine.lock() {
                eng.weather = w;
            }
        }
        thread::sleep(WEATHER_INTERVAL);
    });

    let css_path = get_css_output_path();
    if let Some(parent) = css_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut last_hex = String::new();
    loop {
        let hex = match engine.lock() {
            Ok(eng) => eng.current_hex(),
            Err(_) => {
                thread::sleep(COLOR_INTERVAL);
                continue;
            }
        };

        if hex != last_hex {
            write_css_atomic(&css_path, &hex);
            last_hex = hex;
        }

        thread::sleep(COLOR_INTERVAL);
    }
}
