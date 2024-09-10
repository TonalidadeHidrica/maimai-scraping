use derive_by_key::DeriveByKey;
use hashbrown::HashMap;
use itertools::Itertools;
use getset::Getters;

use crate::maimai::schema::latest::SongIcon;

use super::Song;

#[derive(Getters)]
#[getset(get = "pub")]
pub struct SongDatabase<'a> {
    songs: Vec<SongRef<'a>>,
    icon_map: HashMap<&'a SongIcon, SongRef<'a>>,
}
impl<'a> SongDatabase<'a> {
    pub fn new(songs: &'a [Song]) -> Self {
        let songs = songs
            .iter()
            .enumerate()
            .map(|(id, song)| SongRef { song, id })
            .collect_vec();

        // Make icon map.
        // `verify_properties` guarantees that an icon exists for all unremoved songs.
        let icon_map = songs
            .iter()
            .filter_map(|&x| Some((x.song.icon.as_ref()?, x)))
            .collect();

        Self { songs, icon_map }
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
