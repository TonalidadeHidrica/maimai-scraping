use derive_by_key::DeriveByKey;

use super::Song;
// use std::cmp::Par

pub struct SongDatabase<'a> {
    songs: Vec<SongRef<'a>>,
}
impl<'a> SongDatabase<'a> {
    pub fn new(songs: &'a [Song]) -> Self {
        let songs = songs
            .iter()
            .enumerate()
            .map(|(id, song)| SongRef { song, id })
            .collect();
        // let mut icon_map = songs.iter().map(|x| (&x.icon));
        Self { songs }
    }
}

#[derive(Clone, Copy, DeriveByKey)]
#[derive_by_key(key = "key", PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SongRef<'a> {
    song: &'a Song,
    id: usize,
}
impl SongRef<'_> {
    fn key(&self) -> usize {
        self.id
    }
}
