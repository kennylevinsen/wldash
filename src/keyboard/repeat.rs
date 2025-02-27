use std::time::Duration;

use calloop::{
    channel::{self, Channel},
    timer::{TimeoutAction, Timer},
    EventSource, Poll, PostAction, Readiness, Token, TokenFactory,
};
use wayland_client::{protocol::wl_keyboard, WEnum};

use super::KeyEvent;

#[derive(Debug)]
pub enum RepeatMessage {
    StopRepeat,
    KeyEvent(KeyEvent),
    RepeatInfo((u32, u32)),
}

#[derive(Debug)]
pub struct KeyRepeatSource {
    channel: Channel<RepeatMessage>,
    timer: Timer,
    rate: Duration,
    delay: Duration,
    disabled: bool,
    key: Option<KeyEvent>,
}

impl KeyRepeatSource {
    pub fn new(ch: channel::Channel<RepeatMessage>) -> KeyRepeatSource {
        KeyRepeatSource {
            channel: ch,
            timer: Timer::immediate(),
            delay: Duration::ZERO,
            rate: Duration::ZERO,
            disabled: true,
            key: None,
        }
    }
}

impl EventSource for KeyRepeatSource {
    type Event = KeyEvent;
    type Metadata = ();
    type Ret = ();
    type Error = calloop::Error;

    fn process_events<F>(
        &mut self,
        readiness: Readiness,
        token: Token,
        mut callback: F,
    ) -> calloop::Result<PostAction>
    where
        F: FnMut(Self::Event, &mut Self::Metadata) -> Self::Ret,
    {
        let mut removed = false;

        let timer = &mut self.timer;
        let rate = &mut self.rate;
        let delay = &mut self.delay;
        let key = &mut self.key;
        let disabled = &mut self.disabled;

        let mut reregister = false;

        let now = std::time::Instant::now();

        // Check if the key repeat should stop
        let channel_pa = self
            .channel
            .process_events(readiness, token, |event, _| match event {
                channel::Event::Msg(message) => match message {
                    RepeatMessage::StopRepeat => {
                        key.take();
                    }
                    RepeatMessage::KeyEvent(event) => match event.state {
                        WEnum::Value(wl_keyboard::KeyState::Pressed) => {
                            key.replace(event);
                            reregister = true;
                            timer.set_deadline(now + *delay);
                        }
                        WEnum::Value(wl_keyboard::KeyState::Released) => match key {
                            Some(k) if k.keysym == event.keysym => {
                                key.take();
                            }
                            _ => (),
                        },
                        _ => (),
                    },
                    RepeatMessage::RepeatInfo((new_rate, new_delay)) => {
                        *rate = Duration::from_micros(1_000_000 / new_rate as u64);
                        *delay = Duration::from_millis(new_delay as u64);
                        *disabled = false;
                        if key.is_some() {
                            timer.set_deadline(now + *delay);
                        }
                    }
                },

                channel::Event::Closed => {
                    removed = true;
                }
            })
            .map_err(|err| calloop::Error::OtherError(Box::new(err)))?;

        // Keyboard was destroyed
        if removed {
            return Ok(PostAction::Remove);
        }

        // Re-register the timer to start it again
        if reregister {
            return Ok(PostAction::Reregister);
        }

        let timer_pa = timer.process_events(readiness, token, |deadline, _| {
            if self.disabled || key.is_none() {
                return TimeoutAction::Drop;
            }
            if deadline - now > std::time::Duration::from_millis(5) {
                // We have not yet reached the deadline within our tolerance
                return TimeoutAction::ToInstant(deadline);
            }

            // Invoke the event
            callback(key.as_ref().unwrap().clone(), &mut ());

            let mut next_deadline = deadline;
            if now - deadline > *rate * 2 {
                // We're way behind, let's just set a new time
                println!("KeyRepeatSource::process_events: More than two repeat cycles behind, resetting");
                next_deadline = now + *rate;
            } else {
                while next_deadline < now {
                    next_deadline += *rate;
                }
            }
            TimeoutAction::ToInstant(next_deadline)
        })?;

        // Only disable or remove if both want to, otherwise continue or re-register
        Ok(match (timer_pa, channel_pa) {
            (PostAction::Disable, PostAction::Disable) => PostAction::Disable,
            (PostAction::Remove, PostAction::Remove) => PostAction::Remove,
            (PostAction::Reregister, _) | (_, PostAction::Reregister) => PostAction::Reregister,
            _ => PostAction::Continue,
        })
    }

    fn register(
        &mut self,
        poll: &mut Poll,
        token_factory: &mut TokenFactory,
    ) -> calloop::Result<()> {
        self.channel.register(poll, token_factory)?;
        self.timer.register(poll, token_factory)
    }

    fn reregister(
        &mut self,
        poll: &mut Poll,
        token_factory: &mut TokenFactory,
    ) -> calloop::Result<()> {
        self.channel.reregister(poll, token_factory)?;
        self.timer.reregister(poll, token_factory)
    }

    fn unregister(&mut self, poll: &mut Poll) -> calloop::Result<()> {
        self.channel.unregister(poll)?;
        self.timer.unregister(poll)
    }
}
