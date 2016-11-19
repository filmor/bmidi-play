extern crate bmidi;

pub trait Sink {
    fn process_event(&mut self, evt: &bmidi::Event);
}
