use crossterm::{
    cursor::Hide,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{Clear, ClearType},
    ExecutableCommand, Result,
};
use pulldown_cmark::{html, Options, Parser};
use std::io::{self, Read};
use std::io::{stdout, Write};

fn main() -> Result<()> {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    // println!("{:?}", buffer);
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(&buffer, options);
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    term::write(&mut handle, parser)?;
    // stdout()
    //     // .execute(Clear(ClearType::All))?
    //     .execute(SetForegroundColor(Color::Blue))?
    //     .execute(SetBackgroundColor(Color::Red))?
    //     .execute(Print("Styled text here."))?
    //     .execute(ResetColor)?;
    Ok(())
}

mod term {
    use crossterm::{
        cursor::Hide,
        style::{
            Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor,
            SetForegroundColor,
        },
        terminal::{Clear, ClearType},
        ExecutableCommand, Result,
    };
    use pulldown_cmark::Event::*;
    use pulldown_cmark::{Event, Tag};
    use std::collections::HashMap;
    use std::fmt::{Arguments, Write as FmtWrite};
    use std::io::{self, ErrorKind, Write};

    struct TermWriter<I, W> {
        /// Iterator supplying events.
        iter: I,

        /// Writer to write to.
        writer: W,
    }

    impl<'a, I, W> TermWriter<I, W>
    where
        I: Iterator<Item = Event<'a>>,
        W: Write,
    {
        fn new(iter: I, writer: W) -> Self {
            Self { iter, writer }
        }

        // #[inline]
        // fn write(&mut self, s: &str) -> io::Result<()> {
        //     self.writer.write_all(s.as_bytes())?;

        //     if !s.is_empty() {
        //         // self.end_newline = s.ends_with('\n');
        //     }
        //     Ok(())
        // }

        fn encode_image(&self, url: &str) -> io::Result<String> {
            use std::str::FromStr;
            let mut response =
                reqwest::get(url).map_err(|e| io::Error::from(io::ErrorKind::Other))?;
            let headers = response.headers().clone();

            let mut buf: Vec<u8> = vec![];
            response
                .copy_to(&mut buf)
                .map_err(|e| io::Error::from(io::ErrorKind::Other))?;

            match headers
                .get(reqwest::header::CONTENT_TYPE)
                .ok_or(()) // todo
                .and_then(|content_type| {
                    let content_type = mime::Mime::from_str(content_type.to_str().map_err(|e| ())?)
                        .map_err(|e| ())?;
                    Ok(content_type)
                }) {
                Ok(m) if m.subtype() == mime::SVG => {
                    use io::Read;
                    use resvg::prelude::*;
                    let backend = resvg::default_backend();
                    let mut opt = resvg::Options::default();
                    let rtree = usvg::Tree::from_data(&buf, &opt.usvg)
                        .map_err(|e| io::Error::from(io::ErrorKind::Other))?;
                    let mut img = backend
                        .render_to_image(&rtree, &opt)
                        .ok_or(io::Error::from(io::ErrorKind::Other))?;
                    let mut tmp = tempfile::NamedTempFile::new()?;
                    // tempfile::tempfile();
                    // let tmp = tmp.into_temp_path();
                    let saved = img.save_png(&tmp.path());
                    let mut buf: Vec<u8> = vec![];
                    tmp.read_to_end(&mut buf)?;
                    Ok(base64::encode(&buf))
                }
                Ok(m) if m.type_() == mime::IMAGE => Ok(base64::encode(&buf)),
                Err(()) => Ok(base64::encode(&buf)),
                _ => Ok(base64::encode(&buf)),
            }
        }

        fn style(&mut self, command: impl Command<AnsiType = String>) -> io::Result<()> {
            self.writer.execute(command);
            Ok(())
        }

        fn write_str(&mut self, s: &str) -> io::Result<()> {
            self.writer.write_all(s.as_bytes())
        }

        fn consume_to_end(&mut self) -> io::Result<String> {
            let mut nest = 0;
            let mut out = vec![];
            while let Some(event) = self.iter.next() {
                match event {
                    Start(_) => nest += 1,
                    Text(t) => out.push(t),
                    End(_) => {
                        if nest == 0 {
                            break;
                        }
                        nest -= 1;
                    }
                    _ => {}
                }
            }
            Ok(out.join(" "))
        }

        pub fn run(mut self) -> io::Result<()> {
            while let Some(event) = self.iter.next() {
                match event {
                    Start(tag) => match tag {
                        Tag::Emphasis => self.style(SetAttribute(Attribute::Bold))?,
                        Tag::Strong => self.style(SetAttribute(Attribute::Bold))?,
                        Tag::Paragraph => self.write_str("\n")?,
                        Tag::Strikethrough => self.style(SetAttribute(Attribute::CrossedOut))?,
                        Tag::Link(ty, url, title) => {}
                        Tag::Heading(level) => self.style(SetAttribute(Attribute::Bold))?,
                        Tag::Image(ty, url, title) => {
                            if let Ok(img) = self.encode_image(&url) {
                                self.writer.write_all(&[0x1b, 0x5d])?;
                                self.write_str(&format!(
                                    "1337;File=name={};inline=1;height=1:{}",
                                    title, img
                                ))?;
                                self.writer.write_all(&[0x07])?;
                                self.write_str(" ")?;
                                self.consume_to_end()?;
                            };
                        }
                        Tag::CodeBlock(language) => {
                            use syntect::easy::HighlightLines;
                            use syntect::highlighting::{Style, ThemeSet};
                            use syntect::parsing::SyntaxSet;
                            use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};
                            let ps = SyntaxSet::load_defaults_newlines();
                            let ts = ThemeSet::load_defaults();
                            let lcn = language.as_ref().to_lowercase();
                            let syntax = ps
                                .find_syntax_by_extension(&language)
                                .or_else(|| {
                                    let mapped_language = match lcn.as_ref() {
                                        "jsx" => "JavaScript",
                                        "scala" => "Scala",
                                        other => &other,
                                    };
                                    // println!(
                                    //     "{:?}",
                                    //     ps.syntaxes()
                                    //         .iter()
                                    //         .map(|s| s.name.clone())
                                    //         .collect::<Vec<String>>()
                                    // );
                                    ps.find_syntax_by_name(&mapped_language)
                                })
                                .unwrap_or_else(|| ps.find_syntax_plain_text());
                            let mut h =
                                HighlightLines::new(syntax, &ts.themes["base16-ocean.dark"]);
                            let code = self.consume_to_end()?;
                            for line in LinesWithEndings::from(&code) {
                                // LinesWithEndings enables use of newlines mode
                                let ranges: Vec<(Style, &str)> = h.highlight(line, &ps);
                                for &(ref style, text) in ranges.iter() {
                                    self.style(SetForegroundColor(Color::Rgb {
                                        r: style.foreground.r,
                                        g: style.foreground.g,
                                        b: style.foreground.b,
                                    }))?;
                                    // self.style(SetBackgroundColor(Color::Rgb {
                                    //     r: style.background.r,
                                    //     g: style.background.g,
                                    //     b: style.background.b,
                                    // }))?;
                                    self.write_str(text)?;
                                }
                            }
                            self.style(ResetColor)?;
                            self.write_str("\n")?;
                        }
                        Tag::List(_) => {}
                        Tag::Item => {
                            self.write_str("\n\t* ")?;
                        }
                        other => {
                            print!("[todo:{:?}]", other);
                        }
                    },
                    Text(s) => self.write_str(&s)?,
                    End(tag) => match tag {
                        Tag::Item => (),
                        Tag::Paragraph => self.write_str("\n")?,
                        Tag::Heading(level) => self.write_str("\n")?,
                        Tag::List(_) => self.write_str("\n")?,
                        _ => self.style(ResetColor)?,
                    },
                    Code(s) => {
                        self.style(SetBackgroundColor(Color::DarkGrey))?;
                        self.write_str(&s)?;
                        self.style(ResetColor)?;
                    }
                    _ => self.write_str("hi")?,
                }
            }
            Ok(())
        }
    }

    struct WriteWrapper<W>(W);

    use crossterm::Command;

    // impl<W> StrWrite for &'_ mut W
    // where
    //     W: StrWrite,
    // {
    //     #[inline]
    //     fn write_str(&mut self, s: &str) -> io::Result<()> {
    //         (**self).write_str(s)
    //     }

    //     #[inline]
    //     fn write_fmt(&mut self, args: Arguments) -> io::Result<()> {
    //         (**self).write_fmt(args)
    //     }
    // }

    pub fn write<'a, I, W>(writer: W, iter: I) -> io::Result<()>
    where
        I: Iterator<Item = Event<'a>>,
        W: Write,
    {
        TermWriter::new(iter, writer).run()
    }
}
