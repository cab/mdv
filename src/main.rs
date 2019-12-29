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

        fn consume_to_end(&mut self) -> io::Result<()> {
            let mut nest = 0;
            while let Some(event) = self.iter.next() {
                match event {
                    Start(_) => nest += 1,
                    End(_) => {
                        if nest == 0 {
                            break;
                        }
                        nest -= 1;
                    }
                    _ => {}
                }
            }
            Ok(())
        }

        pub fn run(mut self) -> io::Result<()> {
            while let Some(event) = self.iter.next() {
                match event {
                    Start(tag) => match tag {
                        Tag::Emphasis => self.style(SetAttribute(Attribute::Bold))?,
                        Tag::Paragraph => self.write_str("\n")?,
                        Tag::Strikethrough => self.style(SetAttribute(Attribute::CrossedOut))?,
                        Tag::Link(ty, url, title) => {}
                        Tag::Heading(level) => self.style(SetAttribute(Attribute::Bold))?,
                        Tag::Image(ty, url, title) => {
                            self.writer.write_all(&[0x1b, 0x5d])?;
                            self.write_str(&format!(
                                "1337;File=name={};inline=1;height=1:{}",
                                title,
                                self.encode_image(&url)?
                            ))?;
                            self.writer.write_all(&[0x07])?;
                            self.write_str(" ")?;
                            self.consume_to_end()?;
                            //     println!("lol {}", "\x1b]1337;File=inline=1:iVBORw0KGgoAAAANSUhEUgAAABwAAAAcCAYAAAByDd+UAAAAGXRFWHRTb2Z0d2FyZQBBZG9iZSBJbWFnZVJlYWR5ccllPAAABQtJREFUeNrsVktoXVUUXefc7/vmpUmaNonWSsFYtBZEWhRKERxI60QF6UhBOhAVEXEmzjrVQbXgTBRFB4oTlSq1FEGEiggWlKpVjG0wn9fXvE/u75zjOucmaVIrWhBB6IX9eO9+9tpr7bX3fcIYg//yENcB/33As+PX9oStr8GPvsClJ0dG1AXviIiKSYyJ1+SucFEI3KqXknFZHxpH7n0sW9Xj8UPjEA0Jk2j41wBU5+ejaOnH0NChansLmMjugB+MeDs0REsclGEC4ZHFuEAxPwOVNYfjA5PH5UgAfSnnhX/KUOFZNPWLjFZxxkd6OoKeq0COePCaPkTsQZKaKgCtDALSyM9eBG65+WLlwelZ9Usn5A0xM7X9v5VPYD8m1Ut6VmD5/RrUuRpkVEFwk4RsSRbDsgvhqi9yTVCF0LOPSohMDZtePkyw1YxTPmR+ZVft3SGKMEOEOzFanEw/D5B+2oTwa4h3kVGF/cgJVjDIzDbG1hZ7BkZJELY8f9mQbcZeRubPZY2N6hkPkTGjm0Y7J1AV04N3qshPDyHcWYE/YRvvwRSk4Et4nge9klMbjbzQ8KQpi4ZYn3aYcR/jmP9Jd3KVmtPQaIV+rfnEnu789O435hfUr63Ryt0VWJ1MSiA2SHgEFhLLJBJaYD5qpUyJXvM9mwXrZFxN/irjqB/IjaYxYfyAbuoXsjeX5nC+IaJ9MaSxmnlQTOZZQMnvKwWSEiU08HhLg8gqZ4uMWIezdhxjvEdz1VFGlW0OH1/YfO9bO0/UsGfu583FvuqYVAGWEoEBAYM4RGYkUiaUBK/XAgQBGdl+ybKfQogVHLHOee44xfiMd1nT5BA6fbk7Wj+8+dv58/d8dRrYvQlKhTBkE0ZlYsGkmRbIGT4l7nKQ+xmHmeCSEs4spjynqLq8muffZfTXxqKIw0T01I37Tr4+FGxPkcQTkLS8Zq+qse8YMTea1cAx0dqacEVWUfJoWcYUW+niaoBzjC8doGCzl4Yrvds/PLt1S95HOjUBn89IslIETHODPFeoVUO62NCgJYNGxXPWL2gYwxytGn8XBkl21f18yEpqAR/JqsFv9Zn+/TvPsYgdQxxNgX5hZ0pjqBGUgBouuUfp+knuxqFe8Z073Xmy7A0K5FmOZmyXk7nSOEcYJ32+Lp7rN6O7bvvivGqpZaRDY24XWFdra3eCxmRaZxRORzt0Tk9Y5TxXh3bFkSacZ9cGfgNTO/h7pfLlN1E7w7bZRQ9bIyCBq74SCtQj4QC5ILHUzVg96y4KWCUbvC6UQq+fIUncEnWbxp7XtiD9J1mfsXR9ynl40/cdjA36UDfUWKmhJ4wDEsatdyY2iKxRCu12pKGKqbK95HgovcbEskzygm62U7oBkJscH9ldKgfV8FS8nCDUuZPQVtfp8TtNYte/pG5JkqHTTZicTPLMbmmolIyTFFUm5+Llqyh392v2VOelvG6fXl5tPzBm2HWzf9tPS0eNNE8hZPPZg0bAyr3CDbOdL9uvZcrXFNyVXjkSlbgkVth+uZk3rndxZLePRiapUOQtilpwDgsmWnk9dcSPbx/A9u/ajV7S+VqExQ52plz0q6LYfAQNCJQpg7/+R7JygTdwkvjCzWDioVeqD08/LbfEMINyNv3ts33o0WrX/J4/X1xoHyRMzmrW0ppyrt0rT7On7rcwV+xls3ECjHsuxuLgg/xMG/HUVLn+bDHX/yb+7wH/EGAARjZ2jNWjuZgAAAAASUVORK5CYII=");
                            //     self.writer.write_str(&format!(
                            //     "\x1b]1337;File=inline=1:iVBORw0KGgoAAAANSUhEUgAAABwAAAAcCAYAAAByDd+UAAAAGXRFWHRTb2Z0d2FyZQBBZG9iZSBJbWFnZVJlYWR5ccllPAAABQtJREFUeNrsVktoXVUUXefc7/vmpUmaNonWSsFYtBZEWhRKERxI60QF6UhBOhAVEXEmzjrVQbXgTBRFB4oTlSq1FEGEiggWlKpVjG0wn9fXvE/u75zjOucmaVIrWhBB6IX9eO9+9tpr7bX3fcIYg//yENcB/33As+PX9oStr8GPvsClJ0dG1AXviIiKSYyJ1+SucFEI3KqXknFZHxpH7n0sW9Xj8UPjEA0Jk2j41wBU5+ejaOnH0NChansLmMjugB+MeDs0REsclGEC4ZHFuEAxPwOVNYfjA5PH5UgAfSnnhX/KUOFZNPWLjFZxxkd6OoKeq0COePCaPkTsQZKaKgCtDALSyM9eBG65+WLlwelZ9Usn5A0xM7X9v5VPYD8m1Ut6VmD5/RrUuRpkVEFwk4RsSRbDsgvhqi9yTVCF0LOPSohMDZtePkyw1YxTPmR+ZVft3SGKMEOEOzFanEw/D5B+2oTwa4h3kVGF/cgJVjDIzDbG1hZ7BkZJELY8f9mQbcZeRubPZY2N6hkPkTGjm0Y7J1AV04N3qshPDyHcWYE/YRvvwRSk4Et4nge9klMbjbzQ8KQpi4ZYn3aYcR/jmP9Jd3KVmtPQaIV+rfnEnu789O435hfUr63Ryt0VWJ1MSiA2SHgEFhLLJBJaYD5qpUyJXvM9mwXrZFxN/irjqB/IjaYxYfyAbuoXsjeX5nC+IaJ9MaSxmnlQTOZZQMnvKwWSEiU08HhLg8gqZ4uMWIezdhxjvEdz1VFGlW0OH1/YfO9bO0/UsGfu583FvuqYVAGWEoEBAYM4RGYkUiaUBK/XAgQBGdl+ybKfQogVHLHOee44xfiMd1nT5BA6fbk7Wj+8+dv58/d8dRrYvQlKhTBkE0ZlYsGkmRbIGT4l7nKQ+xmHmeCSEs4spjynqLq8muffZfTXxqKIw0T01I37Tr4+FGxPkcQTkLS8Zq+qse8YMTea1cAx0dqacEVWUfJoWcYUW+niaoBzjC8doGCzl4Yrvds/PLt1S95HOjUBn89IslIETHODPFeoVUO62NCgJYNGxXPWL2gYwxytGn8XBkl21f18yEpqAR/JqsFv9Zn+/TvPsYgdQxxNgX5hZ0pjqBGUgBouuUfp+knuxqFe8Z073Xmy7A0K5FmOZmyXk7nSOEcYJ32+Lp7rN6O7bvvivGqpZaRDY24XWFdra3eCxmRaZxRORzt0Tk9Y5TxXh3bFkSacZ9cGfgNTO/h7pfLlN1E7w7bZRQ9bIyCBq74SCtQj4QC5ILHUzVg96y4KWCUbvC6UQq+fIUncEnWbxp7XtiD9J1mfsXR9ynl40/cdjA36UDfUWKmhJ4wDEsatdyY2iKxRCu12pKGKqbK95HgovcbEskzygm62U7oBkJscH9ldKgfV8FS8nCDUuZPQVtfp8TtNYte/pG5JkqHTTZicTPLMbmmolIyTFFUm5+Llqyh392v2VOelvG6fXl5tPzBm2HWzf9tPS0eNNE8hZPPZg0bAyr3CDbOdL9uvZcrXFNyVXjkSlbgkVth+uZk3rndxZLePRiapUOQtilpwDgsmWnk9dcSPbx/A9u/ajV7S+VqExQ52plz0q6LYfAQNCJQpg7/+R7JygTdwkvjCzWDioVeqD08/LbfEMINyNv3ts33o0WrX/J4/X1xoHyRMzmrW0ppyrt0rT7On7rcwV+xls3ECjHsuxuLgg/xMG/HUVLn+bDHX/yb+7wH/EGAARjZ2jNWjuZgAAAAASUVORK5CYII=",
                            // ))?
                        }
                        other => {
                            print!("[todo:{:?}]", other);
                        }
                    },
                    Text(s) => self.write_str(&s)?,
                    End(tag) => match tag {
                        Tag::Paragraph => self.write_str("\n")?,
                        Tag::Heading(level) => self.write_str("\n")?,
                        _ => self.style(ResetColor)?,
                    },
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
