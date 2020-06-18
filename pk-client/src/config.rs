
use serde::{Serialize, Deserialize};
use super::Error;
use runic::Color;

fn color_from_hex(h: &str) -> Result<Color, std::num::ParseIntError> {
    let start = if h.chars().next() == Some('#') { 1 } else { 0 };
    let h = &h[start..];
    let r = u8::from_str_radix(&h[0..2], 16)? as f32 / 255.0;
    let g = u8::from_str_radix(&h[2..4], 16)? as f32 / 255.0;
    let b = u8::from_str_radix(&h[4..6], 16)? as f32 / 255.0;
    let a = if h.len() > 6 {
        u8::from_str_radix(&h[6..9], 16)? as f32 / 255.0
    } else {
        1.0
    };
    Ok(Color::rgba(r,g,b,a))
}

#[derive(Clone, Debug)]
pub struct Colorscheme {
    pub background: Color,
    pub quarter_gray: Color,
    pub half_gray: Color,
    pub three_quarter_gray: Color,
    pub foreground: Color,

    pub accent: [Color; 7]
}

impl Default for Colorscheme {
    fn default() -> Self {
        Colorscheme {
            background: Color::black(),
            quarter_gray: Color::rgb(0.25, 0.25, 0.25),
            half_gray: Color::rgb(0.5, 0.5, 0.5),
            three_quarter_gray: Color::rgb(0.75, 0.75, 0.75),
            foreground: Color::white(),

            accent: [
                color_from_hex("ff2800").unwrap(), //red
                color_from_hex("ff9a21").unwrap(), //orange
                color_from_hex("ffdc00").unwrap(), //yellow
                color_from_hex("00ff97").unwrap(), //green
                color_from_hex("3ff2ee").unwrap(), //aqua
                color_from_hex("3fc2ff").unwrap(), //blue
                color_from_hex("9000ff").unwrap(), //purple
            ]
        }
    }
}

impl Colorscheme {
    fn from_toml(val: &toml::Value) -> Result<Colorscheme, super::Error> {
        use toml::Value;
        let background = val.get("background").and_then(Value::as_str)
                        .ok_or_else(|| Error::ConfigParseError("Expected color scheme to have background".into(), Some(val.clone())))
                        .and_then(|s| color_from_hex(s).map_err(Error::from_other))?;
        let foreground = val.get("foreground").and_then(Value::as_str)
                        .ok_or_else(|| Error::ConfigParseError("Expected color scheme to have foreground".into(), Some(val.clone())))
                        .and_then(|s| color_from_hex(s).map_err(Error::from_other))?;

        let quarter_gray = val.get("quarter-gray").and_then(Value::as_str)
                        .map_or_else(|| Ok(background.mix(foreground, 0.25)), |s| color_from_hex(s).map_err(Error::from_other))?;
        let half_gray = val.get("half-gray").and_then(Value::as_str)
                        .map_or_else(|| Ok(background.mix(foreground, 0.5)), |s| color_from_hex(s).map_err(Error::from_other))?;
        let three_quarter_gray = val.get("three-quarter-gray").and_then(Value::as_str)
                        .map_or_else(|| Ok(background.mix(foreground, 0.75)), |s| color_from_hex(s).map_err(Error::from_other))?;
        
        let mut cs = Colorscheme { 
            background, foreground, quarter_gray, half_gray, three_quarter_gray,
            accent: [Color::black(); 7]
        };

        for (i, v) in val.get("accents").and_then(Value::as_array)
            .ok_or_else(|| Error::ConfigParseError("Expected color scheme to have accent colors".into(), Some(val.clone())))?
            .iter().enumerate()
        {
            let col = v.as_str()
                .ok_or_else(|| Error::ConfigParseError(format!("Expected color scheme to have valid accent at #{}", i), Some(v.clone())))
                .and_then(|s| color_from_hex(s).map_err(Error::from_other))?;
            cs.accent[i] = col;
        }
        
        Ok(cs)
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub autoconnect_servers: Vec<(String, String)>,
    pub font: (String, f32),
    pub colors: Colorscheme
}

impl Config {
    pub fn from_toml(val: toml::Value) -> Result<Config, super::Error> {
        use toml::Value;
        let mut cfg = Config::default();
        if val.get("no-local-server").and_then(Value::as_bool).unwrap_or(false) {
            cfg.autoconnect_servers.pop();
        }
        if let Some(s) = val.get("autoconnect").and_then(Value::as_array) {
            for srv in s.iter() {
                cfg.autoconnect_servers.push((
                    srv.get("name").and_then(Value::as_str)
                        .ok_or_else(|| Error::ConfigParseError("Expected server connection to have 'name' field".into(), Some(srv.clone())))?.into(),
                    srv.get("url").and_then(Value::as_str)
                        .ok_or_else(|| Error::ConfigParseError("Expected server connection to have 'url' field".into(), Some(srv.clone())))?.into()
                ));
            }
        }
        if let Some(f) = val.get("font").and_then(Value::as_table) {
            cfg.font = (
                f.get("name").and_then(Value::as_str)
                    .ok_or_else(|| Error::ConfigParseError("Expected font name".into(), val.get("font").cloned()))?.into(),
                f.get("size").and_then(Value::as_float)
                    .ok_or_else(|| Error::ConfigParseError("Expected font size".into(), val.get("font").cloned()))? as f32
            );
        }
        if let Some(ct) = val.get("colors") {
            cfg.colors = Colorscheme::from_toml(ct)?;
        }
        Ok(cfg)
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            autoconnect_servers: vec![("local".into(), "ipc://pk".into())],
            font: ("Fira Code".into(), 14.0),
            colors: Colorscheme::default()
        }
    }
}
