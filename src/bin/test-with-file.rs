use clap::{Arg, App};

fn main() {
    let matches = App::new("hoge").arg(Arg::with_name("file").required(true)).get_matches();
    println!("{:?}", matches.value_of("file"));

}
