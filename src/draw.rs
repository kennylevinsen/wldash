use crate::color::Color;
use crate::buffer::Buffer;
use rusttype::{point, Font, Scale};
use chrono::{Date, DateTime, Datelike, Local, Timelike};

static DEJAVUSANS_MONO: &'static [u8] = include_bytes!("../fonts/dejavu/DejaVuSansMono.ttf");
static ROBOTO_REGULAR: &'static [u8] = include_bytes!("../fonts/Roboto-Regular.ttf");

fn draw_text(
    font_data: &'static [u8],
    buf: &mut Buffer,
    background_color: &Color,
    color: &Color,
    size: f32,
    s: &str,
) -> Result<(), ::std::io::Error> {
    // Load the font
    // This only succeeds if collection consists of one font
    let font = Font::from_bytes(font_data as &[u8]).expect("Error constructing Font");

    // The font size to use
    let scale = Scale::uniform(size);

    let v_metrics = font.v_metrics(scale);

    // layout the glyphs in a line with 20 pixels padding
    let glyphs: Vec<_> = font
        .layout(s, scale, point(20.0, 20.0 + v_metrics.ascent))
        .collect();

    // Loop through the glyphs in the text, positing each one on a line
    for glyph in glyphs {
        if let Some(bounding_box) = glyph.pixel_bounding_box() {
            // Draw the glyph into the image per-pixel by using the draw closure
            glyph.draw(|x, y, o| {
                let x = x + bounding_box.min.x as u32;
                let y = y + bounding_box.min.y as u32;
                let o = if o > 1.0 {
                    1.0
                } else if o < 0.0 {
                    0.0
                } else {
                    o
                };
                buf.put((x, y), &background_color.blend(color, o));
            });
        }
    }

    Ok(())
}

fn draw_text_fixed_width(
    font_data: &'static [u8],
    buf: &mut Buffer,
    background_color: &Color,
    color: &Color,
    size: f32,
    distances: Vec<u32>,
    s: &str,
) -> Result<(), ::std::io::Error> {
    // Load the font
    // This only succeeds if collection consists of one font
    let font = Font::from_bytes(font_data as &[u8]).expect("Error constructing Font");

    // The font size to use
    let scale = Scale::uniform(size);

    let v_metrics = font.v_metrics(scale);

    // layout the glyphs in a line with 20 pixels padding
    let glyphs: Vec<_> = font
        .layout(s, scale, point(20.0, 20.0 + v_metrics.ascent))
        .collect();

    // Loop through the glyphs in the text, positing each one on a line
    let mut x_pos: usize = 0;
    let mut x_off: u32 = 0;
    for glyph in glyphs {
        if let Some(bounding_box) = glyph.pixel_bounding_box() {
            let x_dist = distances[x_pos];
            let width = (bounding_box.max.x - bounding_box.min.x) as u32;
            let offset = (x_dist - width) / 2;
            // Draw the glyph into the image per-pixel by using the draw closure
            glyph.draw(|x, y, o| {
                let off = x_off + offset + 20;
                let x = x + off as u32;
                let y = y + bounding_box.min.y as u32;
                let o = if o > 1.0 {
                    1.0
                } else if o < 0.0 {
                    0.0
                } else {
                    o
                };
                buf.put((x, y), &background_color.blend(color, o));
            });
            x_pos += 1;
            x_off += x_dist;
        }
    }

    Ok(())
}

pub fn draw_clock(
    buf: &mut Buffer,
    background_color: &Color,
    time: &DateTime<Local>,
) -> Result<(), ::std::io::Error> {
    draw_text(
        ROBOTO_REGULAR,
        &mut buf.subdimensions((0, 0, 448, 96)),
        background_color,
        &Color::new(1.0, 1.0, 1.0, 1.0),
        64.0,
        &format!(
            "{:?}, {:02}/{:02}/{:4}",
            time.weekday(),
            time.day(),
            time.month(),
            time.year()
        ),
    )?;

    draw_text_fixed_width(
        ROBOTO_REGULAR,
        &mut buf.subdimensions((0, 64, 288*2+64, 256)),
        background_color,
        &Color::new(1.0, 1.0, 1.0, 1.0),
        256.0,
        vec![120, 120, 64, 120, 120],
        &format!("{:02}:{:02}", time.hour(), time.minute()),
    )?;

    Ok(())
}

fn draw_month(
    buf: &mut Buffer,
    background_color: &Color,
    orig: &Date<Local>,
    time: &Date<Local>,
) -> Result<(), ::std::io::Error> {
    let mut time = time.clone();
    let mut y_off = 1;
    let mut done = false;

    let month_str = match time.month() {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => panic!("impossible value"),
    };

    //
    // Draw the of the month
    //

    draw_text(
        ROBOTO_REGULAR,
        &mut buf.subdimensions((0, 0, 364, 96)),
        background_color,
        &Color::new(1.0, 1.0, 1.0, 1.0),
        68.0,
        month_str,
    )?;

    //
    // Draw the week day
    //

    for idx in 1..8 {
        let wk_chr = match idx {
            1 => "MON",
            2 => "TUE",
            3 => "WED",
            4 => "THU",
            5 => "FRI",
            6 => "SUN",
            7 => "SAT",
            _ => panic!("impossible value"),
        };

        draw_text(
            DEJAVUSANS_MONO,
            &mut buf.subdimensions((idx * 48 + 4, (y_off * 32) + 64, 64, 64)),
            background_color,
            &Color::new(0.75, 0.75, 0.75, 1.0),
            16.0,
            &wk_chr,
        )?;
    }

    y_off += 1;

    while !done {
        let mut x_pos = 0;

        //
        // Find the start of this week
        //
        let mut wkday = time.weekday();
        while wkday != chrono::Weekday::Mon {
            x_pos += 1;
            wkday = wkday.pred();
        }

        //
        // Draw the week number
        //
        let wk = time.iso_week();
        draw_text(
            DEJAVUSANS_MONO,
            &mut buf.subdimensions((0 * 48, (y_off * 32) + 64, 64, 64)),
            background_color,
            &Color::new(0.75, 0.75, 0.75, 1.0),
            32.0,
            &format!("{:02}", wk.week()),
        )?;
        x_pos += 1;

        //
        // Draw the dates
        //
        while x_pos < 8 {
            let c = if time.day() == orig.day() && time.month() == orig.month() {
                Color::new(1.0, 1.0, 1.0, 1.0)
            } else {
                Color::new(0.5, 0.5, 0.5, 1.0)
            };
            draw_text(
                DEJAVUSANS_MONO,
                &mut buf.subdimensions((x_pos * 48, (y_off * 32) + 64, 64, 64)),
                background_color,
                &c,
                32.0,
                &format!("{:02}", time.day()),
            )?;
            let t = time.with_day(time.day() + 1);
            if t.is_none() {
                done = true;
                break;
            }
            time = t.unwrap();
            x_pos += 1;
        }

        y_off += 1;
    }

    Ok(())
}

pub fn draw_calendar(
    buf: &mut Buffer,
    background_color: &Color,
    time: &Date<Local>,
) -> Result<(), ::std::io::Error> {
    // ~1546x384px
    let t = time.with_day(1).unwrap();
    draw_month(
        &mut buf.subdimensions((0, 0, 448, 384)),
        background_color,
        time,
        &t.pred().with_day(1).unwrap(),
    )?;
    draw_month(
        &mut buf.subdimensions((512, 0, 448, 384)),
        background_color,
        time,
        &t,
    )?;
    let n = if t.month() == 12 {
        t.with_year(t.year() + 1).unwrap().with_month(1).unwrap()
    } else {
        t.with_month(t.month() + 1).unwrap()
    };
    draw_month(
        &mut buf.subdimensions((1024, 0, 448, 384)),
        background_color,
        time,
        &n,
    )?;
    Ok(())
}