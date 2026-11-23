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
const WAYBAR_CSS: &str = "/etc/xdg/aurore/waybar.css";
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
    bar_visible: bool,
    bar_position: String,
    bar_height: u32,
    launcher_prompt: String,
    launcher_width: u32,
    launcher_lines: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            lat: 35.6895,
            lon: 139.6917,
            base_hue: 0.38,
            saturation_max: 0.55,
            bar_visible: true,
            bar_position: "bottom".into(),
            bar_height: 42,
            launcher_prompt: "Search: ".into(),
            launcher_width: 38,
            launcher_lines: 10,
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

fn home_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
}

fn config_path() -> PathBuf {
    home_dir().join(".config/aurore/aurore.toml")
}

fn cache_dir() -> PathBuf {
    home_dir().join(".cache/aurore")
}

fn load_config() -> Config {
    fs::read_to_string(config_path())
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

// 一時ファイルに書いてから rename するので、読み手が途中の内容を見ない。
fn write_file_atomic(path: &Path, content: &str) {
    let tmp = path.with_extension("tmp");
    if fs::write(&tmp, content).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

// ---- 天気 ----

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

// ---- 色 ----

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

    let (sat, val_mult): (f64, f64) = match weather {
        Weather::Clear => (0.55, 1.00),
        Weather::Cloudy => (0.30, 0.85),
        Weather::Rain => (0.35, 0.72),
        Weather::Snow => (0.20, 1.05),
    };

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

fn write_css(hex: &str) {
    let content = format!("@define-color aurore_primary {};\n", hex);
    write_file_atomic(Path::new(CSS_PATH), &content);
}

fn reload_waybar_css() {
    let _ = Command::new("pkill").args(["-SIGUSR2", "waybar"]).status();
}

// ---- バー（Waybar） ----

fn waybar_json(cfg: &Config) -> String {
    format!(
        r#"{{
    "layer": "top",
    "position": "{pos}",
    "height": {h},
    "margin-bottom": 6,
    "margin-left": 12,
    "margin-right": 12,
    "modules-left": ["wlr/taskbar"],
    "modules-center": ["custom/launcher"],
    "modules-right": ["tray", "clock"],
    "custom/launcher": {{ "format": "⊞", "tooltip": false, "on-click": "aurore-burzellaunch" }},
    "wlr/taskbar": {{ "format": "{{icon}}", "icon-size": 22, "on-click": "activate" }},
    "clock": {{ "format": "{{:%H:%M}}" }}
}}"#,
        pos = cfg.bar_position,
        h = cfg.bar_height,
    )
}

// json が None のときは Waybar を止めるだけにする
fn restart_waybar(json: Option<&str>) {
    let _ = Command::new("pkill").args(["-x", "waybar"]).status();
    if let Some(json) = json {
        let path = cache_dir().join("waybar.jsonc");
        write_file_atomic(&path, json);
        let _ = Command::new("waybar")
            .arg("-c")
            .arg(&path)
            .arg("-s")
            .arg(WAYBAR_CSS)
            .spawn();
    }
}

// ---- ランチャー（fuzzel） ----

fn launcher_ini(cfg: &Config) -> String {
    format!(
        "[main]\n\
         width={}\n\
         lines={}\n\
         anchor=bottom\n\
         y-margin=56\n\
         prompt=\"{}\"\n\
         \n\
         [border]\n\
         width=2\n\
         radius=16\n\
         \n\
         [colors]\n\
         background=121e17f0\n\
         text=e8f3ecff\n\
         selection=243b2eff\n\
         border=4caf50ff\n",
        cfg.launcher_width,
        cfg.launcher_lines,
        cfg.launcher_prompt.replace('"', "'"),
    )
}

fn main() {
    let _ = fs::create_dir_all(cache_dir());

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
                    None => next_attempt = Instant::now() + WEATHER_RETRY,
                }
                last_coords = Some((lat, lon));
            }

            thread::sleep(CONFIG_INTERVAL);
        }
    });

    let mut last_launcher = String::new();
    let mut last_waybar: Option<Option<String>> = None;
    let mut last_hex = String::new();

    loop {
        let cfg = config.lock().map(|c| c.clone()).unwrap_or_default();
        let w = weather.lock().map(|w| *w).unwrap_or(Weather::Clear);

        // ランチャーの設定は変わったときだけ書き出す
        let launcher = launcher_ini(&cfg);
        if launcher != last_launcher {
            write_file_atomic(&cache_dir().join("burzellaunch.ini"), &launcher);
            last_launcher = launcher;
        }

        // バーは設定が変わったときだけ Waybar を作り直す
        let waybar = if cfg.bar_visible {
            Some(waybar_json(&cfg))
        } else {
            None
        };
        if last_waybar.as_ref() != Some(&waybar) {
            restart_waybar(waybar.as_deref());
            last_waybar = Some(waybar);
        }

        // 色は変わったときだけ書き出し、Waybar に読み直させる
        let hex = current_hex(&cfg, w);
        if hex != last_hex {
            write_css(&hex);
            reload_waybar_css();
            last_hex = hex;
        }

        thread::sleep(Duration::from_secs(1));
    }
}
