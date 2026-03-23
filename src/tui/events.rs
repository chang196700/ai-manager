use anyhow::Result;
use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, KeyEventKind};
use std::time::Duration;

pub enum Event {
    Key(KeyEvent),
    Tick,
}

pub fn read_event() -> Result<Event> {
    if event::poll(Duration::from_millis(250))? {
        if let CrosstermEvent::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                return Ok(Event::Key(key));
            }
        }
    }
    Ok(Event::Tick)
}
