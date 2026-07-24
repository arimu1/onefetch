use crate::info::Info;
use ::image::DynamicImage;
use anyhow::{Context, Result};
use onefetch_ascii::AsciiArt;
use onefetch_image::ImageBackend;
use regex::Regex;
use std::fmt::Write as _;
use std::sync::LazyLock;

pub mod factory;

const CENTER_PAD_LENGTH: usize = 3;

#[derive(Clone, clap::ValueEnum, PartialEq, Eq, Debug)]
pub enum SerializationFormat {
    Json,
    Yaml,
}

pub struct Printer {
    info: Info,
    r#type: PrinterType,
    /// Whether color (and other terminal-only escape codes) may be written to the
    /// output. Set to `false` by `--color never`.
    color_enabled: bool,
}

enum PrinterType {
    Plain,
    Json,
    Yaml,
    Ascii {
        art: String,
        no_bold: bool,
    },
    Image {
        image: DynamicImage,
        backend: Box<dyn ImageBackend>,
        resolution: usize,
    },
}

impl Printer {
    pub fn print(&self, writer: &mut dyn std::io::Write) -> Result<()> {
        match &self.r#type {
            PrinterType::Json => {
                write!(writer, "{}", serde_json::to_string_pretty(&self.info)?)?;
                Ok(())
            }
            PrinterType::Yaml => {
                write!(writer, "{}", serde_yaml::to_string(&self.info)?)?;
                Ok(())
            }
            PrinterType::Plain => {
                write_with_line_wrapping(writer, &self.info.to_string(), self.color_enabled)?;
                Ok(())
            }
            PrinterType::Image {
                image,
                backend,
                resolution,
            } => {
                let center_pad = " ".repeat(CENTER_PAD_LENGTH);
                let info_str = self.info.to_string();
                let info_lines = info_str
                    .lines()
                    .map(|s| format!("{center_pad}{s}"))
                    .collect();

                let rendered = backend
                    .add_image(info_lines, image, *resolution)
                    .context("Failed to render image")?;

                write_with_line_wrapping(writer, &rendered, self.color_enabled)?;
                Ok(())
            }
            PrinterType::Ascii { art, no_bold } => {
                let mut buf = String::new();
                let center_pad = " ".repeat(CENTER_PAD_LENGTH);
                let info_str = self.info.to_string();
                let mut info_lines = info_str.lines();
                let mut logo_lines = AsciiArt::new(art, &self.info.ascii_colors, !no_bold);

                loop {
                    match (logo_lines.next(), info_lines.next()) {
                        (Some(logo), Some(info)) => writeln!(buf, "{logo}{center_pad}{info:^}")?,
                        (Some(logo), None) => writeln!(buf, "{logo}")?,
                        (None, Some(info)) => writeln!(
                            buf,
                            "{:<width$}{center_pad}{info:^}",
                            "",
                            width = logo_lines.width()
                        )?,
                        (None, None) => break,
                    }
                }

                write_with_line_wrapping(writer, &buf, self.color_enabled)?;
                Ok(())
            }
        }
    }
}

static ANSI_ESCAPE_CODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1B\[[0-9;]*m").expect("valid regex"));

/// Removes ANSI SGR (color/bold/etc.) escape codes from `content`.
fn strip_ansi_codes(content: &str) -> String {
    ANSI_ESCAPE_CODE_RE.replace_all(content, "").into_owned()
}

fn write_with_line_wrapping(
    writer: &mut dyn std::io::Write,
    content: &str,
    color_enabled: bool,
) -> Result<()> {
    if color_enabled {
        // \x1B[?7l turns off line wrapping and \x1B[?7h turns it on
        write!(writer, "\x1B[?7l{content}\x1B[?7h")?;
    } else {
        write!(writer, "{}", strip_ansi_codes(content))?;
    }
    Ok(())
}

impl PartialEq for PrinterType {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (PrinterType::Plain, PrinterType::Plain)
                | (PrinterType::Json, PrinterType::Json)
                | (PrinterType::Yaml, PrinterType::Yaml)
                | (PrinterType::Ascii { .. }, PrinterType::Ascii { .. })
                | (PrinterType::Image { .. }, PrinterType::Image { .. })
        )
    }
}

impl std::fmt::Debug for PrinterType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            PrinterType::Plain => "Plain",
            PrinterType::Json => "Json",
            PrinterType::Yaml => "Yaml",
            PrinterType::Ascii { .. } => "Ascii",
            PrinterType::Image { .. } => "Image",
        };
        write!(f, "PrinterType::{name}")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_strip_ansi_codes() {
        let colored = "\x1B[31;1mCommits\x1B[0m\x1B[39;1m:\x1B[0m \x1B[39m411\x1B[0m";
        assert_eq!(strip_ansi_codes(colored), "Commits: 411");
    }

    #[test]
    fn test_strip_ansi_codes_without_any_codes() {
        assert_eq!(strip_ansi_codes("Commits: 411"), "Commits: 411");
    }

    #[test]
    fn test_write_with_line_wrapping_color_disabled_strips_codes() {
        let mut buf: Vec<u8> = Vec::new();
        write_with_line_wrapping(&mut buf, "\x1B[31mred\x1B[0m", false).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "red");
    }

    #[test]
    fn test_write_with_line_wrapping_color_enabled_keeps_codes() {
        let mut buf: Vec<u8> = Vec::new();
        write_with_line_wrapping(&mut buf, "\x1B[31mred\x1B[0m", true).unwrap();
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            "\x1B[?7l\x1B[31mred\x1B[0m\x1B[?7h"
        );
    }
}
