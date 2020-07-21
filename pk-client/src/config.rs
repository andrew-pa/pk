
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ColorschemeSel {
    Background,
    QuarterGray,
    HalfGray,
    ThreeQuarterGray,
    Foreground,
    Accent(usize)
}

impl ColorschemeSel {
    pub fn from_toml(val: &toml::Value) -> Result<ColorschemeSel, Error> {
        if val.is_str() {
            Ok(match val.as_str() {
                Some("background") => ColorschemeSel::Background,
                Some("quarter-gray") => ColorschemeSel::QuarterGray,
                Some("half-gray") => ColorschemeSel::HalfGray,
                Some("three-quarter-gray") => ColorschemeSel::ThreeQuarterGray,
                Some("foreground") => ColorschemeSel::Foreground,
                Some(_) => return Err(Error::ConfigParseError("expected a color name".into(), Some(val.clone()))),
                None => panic!()
            })
        } else {
            Ok(ColorschemeSel::Accent(val.as_integer()
                    .ok_or_else(|| Error::ConfigParseError("expected number for accent colorscheme selector".into(), Some(val.clone())))? as usize))
        }
    }
}

#[derive(Clone, Debug)]
pub struct Colorscheme {
    pub background: Color,
    pub quarter_gray: Color,
    pub half_gray: Color,
    pub three_quarter_gray: Color,
    pub foreground: Color,

    pub accent: [Color; 8]
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
                color_from_hex("ff2800").unwrap(), //red 0
                color_from_hex("ff9a21").unwrap(), //orange 1
                color_from_hex("ffdc00").unwrap(), //yellow 2
                color_from_hex("00ff77").unwrap(), //green 3
                color_from_hex("3ff2ee").unwrap(), //aqua 4
                color_from_hex("3fc2ff").unwrap(), //blue 5
                color_from_hex("8000ff").unwrap(), //purple 6
                color_from_hex("c000ff").unwrap(), //magenta 7
            ]
        }
    }
}

impl Colorscheme {
    pub fn get(&self, sel: ColorschemeSel) -> &Color {
        match sel {
            ColorschemeSel::Background => &self.background,
            ColorschemeSel::QuarterGray => &self.quarter_gray,
            ColorschemeSel::HalfGray => &self.half_gray,
            ColorschemeSel::ThreeQuarterGray => &self.three_quarter_gray,
            ColorschemeSel::Foreground => &self.foreground,
            ColorschemeSel::Accent(i) => &self.accent[i],
        }
    }

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
            accent: [Color::black(); 8]
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
    pub tabstop: usize,
    pub softtab: bool,
    pub colors: Colorscheme,
    pub syntax_coloring: Option<toml::Value>
}

impl Config {
    fn default_toml_blob() -> toml::Value {
        toml::toml!{
            syntax-coloring = [
            { scope = "comment", style = "half-gray" },
            { scope = "string", style = 3 },
            { scope = "number, constant", style = 1 },
            { scope = "punctuation", style = "foreground" },
            { scope = "variable", style = "foreground" },
            { scope = "variable.function, entity.name.function", style = 6 },
            { scope = "variable.language", style = 1 },
            { scope = "keyword", style = 0 },
            { scope = "meta.import keyword, keyword.control.import, keyword.other.import", style = 0 },
            { scope = "keyword.operator", style = 4 },
            { scope = "storage", style = 0 },
            { scope = "storage.modifier", style = 3 },
            { scope = "storage.type", style = 0 },
            { scope = "entity.name", style = 6 },
            { scope = "keyword.other.special-method", style = 1 },
            { scope = "keyword.control.class, entity.name, entity.name.class, entity.name.type.class", style = 6 },
            { scope = "support.type", style = 1 },
            { scope = "support, support.class", style = 6 },
            { scope = "meta.path", style = 5 },
            ]
        }
    }

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

        if let Some(ts) = val.get("tabs") {
            use std::convert::TryInto;
            cfg.softtab = ts.get("soft-tab").and_then(|st| st.as_bool()).unwrap_or(cfg.softtab);
            match ts.get("tabstop").and_then(|ts| ts.as_integer()) {
                Some(s) => cfg.tabstop = s.try_into()
                    .map_err(|_| Error::ConfigParseError("Expected positive tabstop value".into(), Some(ts.clone())))?,
                None => {}
            };
        }

        cfg.syntax_coloring = val.get("syntax-coloring").cloned().or_else(|| Config::default_toml_blob().get("syntax-coloring").cloned());
        dbg!(&cfg.syntax_coloring);

        Ok(cfg)
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            autoconnect_servers: vec![("local".into(), "ipc://pk".into())],
            font: ("Fira Code".into(), 14.0),
            tabstop: 4, softtab: true,
            colors: Colorscheme::default(),
            syntax_coloring: Config::default_toml_blob().get("syntax-coloring").cloned()
        }
    }
}
