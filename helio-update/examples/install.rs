//! Installs the latest release of the app into a folder of your choosing
//! the way the app installs an update into its own (see
//! `helio_update::install::rehearse`), to check the installer without
//! touching the installed app or its host:
//!
//!   cargo run -p helio-update --example install -- FOLDER
//!
//! On a Mac, put the release's app of an older version in FOLDER first: the
//! new one must be signed by the same team.

use helio_update::install::rehearse;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [folder] = args.as_slice() else {
        eprintln!("usage: install FOLDER");
        std::process::exit(2);
    };
    let last = std::cell::Cell::new(0u64);
    let result = rehearse(std::path::Path::new(folder), &|done, total| {
        // A line per 10%.
        let step = total / 10 + 1;
        if done / step != last.get() / step || done == total {
            println!("downloaded {done} of {total} bytes");
        }
        last.set(done);
    });
    match result {
        Ok(version) => println!("installed {} {version} in {folder}", helio_update::APP),
        Err(e) => {
            eprintln!("not installed: {e}");
            std::process::exit(1);
        }
    }
}
