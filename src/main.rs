use chrono::{Datelike, Local, Timelike};
use serde::Deserialize;
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

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
    lat: f64,
    lon: f64,
    weather: Weather,
}

impl AuroreEngine {
    fn new(base_hue: f64, lat: f64, lon: f64) -> Self {
        Self {
            base_hue,
            lat,
            lon,
            weather: Weather::Clear,
        }
    }

    fn update_weather(&mut self) {
        let url = format!(
            "https://open-meteo.com{}&longitude={}&current_weather=true",
            self.lat, self.lon
        );
        if let Ok(resp) = ureq::get(&url).call() {
            if let Ok(data) = resp.into_json::<OpenMeteoRes>() {
                self.weather = match data.current_weather.weathercode {
                    0 | 1 => Weather::Clear,
                    2 | 3 | 45 | 48 => Weather::Cloudy,
                    71 | 73 | 75 | 77 | 85 | 86 => Weather::Snow,
                    _ => Weather::Rain,
                };
            }
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

fn main() {
    let engine = Arc::new(Mutex::new(AuroreEngine::new(0.38, 35.6895, 139.6917)));

    let weather_engine = Arc::clone(&engine);
    thread::spawn(move || loop {
        if let Ok(mut eng) = weather_engine.lock() {
            eng.update_weather();
        }
        thread::sleep(Duration::from_secs(900));
    });

    let css_path = get_css_output_path();
    if let Some(parent) = css_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    loop {
        if let Ok(eng) = engine.lock() {
            let hex = eng.current_hex();
            let content = format!("@define-color aurore_primary {};\n", hex);
            let _ = fs::write(&css_path, content);
        }
        thread::sleep(Duration::from_secs(1));
    }
}
