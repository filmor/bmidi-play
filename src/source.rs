extern crate bmidi;
extern crate crossbeam;
extern crate futures;
extern crate time_calc;

use bmidi::{Event, File};
use crossbeam::thread::Scope;
use futures::future::Future;
use futures::sink::Sink;
use std::path::Path;
use time_calc::Ppqn;

pub fn fill_channel<'a, S, E>(scope: &Scope<'a>, tx: S, filename: &'a Path, track: usize)
where
    S: Sink<SinkItem = Event, SinkError = E> + Send + 'a,
    E: Send + 'a,
{
    scope.spawn(move || -> Result<(), E> {
        let mut tx = tx;
        let res = File::parse(filename);
        let track = res.track_iter(track);
        let ppqn = res.division as Ppqn;
        println!("PPQN: {:?}", ppqn);

        for ev in track {
            println!("Sending event {:?}", ev);
            tx = tx.send(ev).wait()?;
            // thread::sleep_ms(ev.delay);
        }

        Ok(())
    });
}
