use crate::buffer::Buffer;
use crate::color::Color;
use crate::module::{ModuleImpl, Input};
use crate::draw::{draw_text, draw_text_fixed_width, ROBOTO_REGULAR};
use chrono::{DateTime, Datelike, Local, Timelike};

pub struct Clock {
	cur_time: DateTime<Local>,
}

impl Clock {
	pub fn new() -> Clock {
		Clock{
			cur_time: Local::now(),
		}
	}
}

impl ModuleImpl for Clock {
	fn draw(&self,
	    buf: &mut Buffer,
	    bg: &Color,
	    time: &DateTime<Local>,
	) -> Result<Vec<(i32, i32, i32, i32)>, ::std::io::Error> {
	    buf.memset(bg);

	    draw_text(
	        ROBOTO_REGULAR,
	        &mut buf.subdimensions((0, 0, 448, 64)),
	        bg,
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
	        &mut buf.subdimensions((0, 64, 288 * 2 + 64, 256)),
	        bg,
	        &Color::new(1.0, 1.0, 1.0, 1.0),
	        256.0,
	        vec![120, 120, 64, 120, 120],
	        &format!("{:02}:{:02}", time.hour(), time.minute()),
	    )?;

	    Ok(vec![buf.get_signed_bounds()])
	}

    fn update(&mut self, time: &DateTime<Local>, force: bool) -> Result<bool, ::std::io::Error> {
    	if time.date() != self.cur_time.date() ||
    		time.hour() != self.cur_time.hour() ||
    		time.minute() != self.cur_time.minute() ||
    		force {
			self.cur_time = time.clone();
			Ok(true)
		} else {
			Ok(false)
		}
    }

	fn input(&mut self, _input: Input) {}
}