use std::collections::HashMap;

use wpdm_common::WpdmMonitor;

pub fn group_by_size(monitors: Vec<WpdmMonitor>) -> impl Iterator<Item = (i32, i32, Vec<String>)> {
    let sizes = monitors.into_iter()
        .fold(HashMap::<(i32, i32), Vec<String>>::new(), |mut init, nxt| {
        if let Some(monitors) = init.get_mut(&(nxt.width, nxt.height)) {
            monitors.push(nxt.name);
        } else {
            init.insert((nxt.width, nxt.height), vec![nxt.name]);
        }
        init
    });
    sizes.into_iter().map(|((width, height), monitors) | (width, height, monitors))
}

