extern crate bmidi;
extern crate crossbeam;
extern crate time_calc;

use bmidi::{Event, File};
use crossbeam::thread::Scope;
use crossbeam_channel::Sender;
use std::path::Path;
use time_calc::Ppqn;

pub fn fill_channel<'a>(scope: &Scope<'a>, tx: Sender<Event>, filename: &'a Path, track: usize) {
    scope.spawn(move || {
        let res = File::parse(filename);
        let track = res.track_iter(track);
        let ppqn = res.division as Ppqn;
        println!("PPQN: {:?}", ppqn);

        for ev in track {
            println!("Sending event {:?}", ev);
            tx.send(ev);
            // thread::sleep_ms(ev.delay);
        }
    });
}
