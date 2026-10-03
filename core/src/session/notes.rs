//! Comments and avatars: the two things riders hang on their own name.
//!
//! A Mumble user may set a comment — a note beside their name, "back in ten",
//! "on the A9 heading north" — and a picture. Both are sent in `UserState`, and
//! both arrive in one of two ways, which is the whole complication:
//!
//! - **Short ones arrive whole.** Under 128 bytes, the server puts the comment
//!   or the image straight in the message.
//! - **Longer ones arrive as a SHA-1 hash**, and the client asks for the body it
//!   wants with a `RequestBlob`. The server then sends another `UserState`
//!   carrying it.
//!
//! The hash is what makes this affordable: a roster of twenty riders with
//! pictures is megabytes, and the hash says whether we already have this one.
//! So a body is asked for once per hash and never again while the value stands —
//! see [`Blobs`].
//!
//! # Comments are HTML
//!
//! Mumble's own client writes comments with a rich-text editor and sends
//! markup. This app has one line of a roster to put it in, and a rider reading
//! `<p>back in ten</p>` at a junction is worse served than one reading nothing,
//! so the markup is stripped down to text here — in the core, once, rather than
//! in each place that draws it.
//!
//! **Stripped, not rendered.** There is no safe way to render a stranger's HTML
//! in this interface and no reason to try: what a comment is for survives as
//! plain text.

use std::collections::{HashMap, HashSet};

/// How many bodies one `RequestBlob` asks for.
///
/// A crowded channel arriving at once would otherwise ask for forty images in
/// one message and then wait on all of them; asking in batches keeps the first
/// few arriving promptly and leaves the rest to the next round.
pub const BLOB_BATCH: usize = 8;

/// Longest comment kept, in characters.
///
/// A comment may be as long as the server's `imagemsglength` allows, which is
/// 128 KiB by default. Nothing here can show more than a line of it, and a
/// roster is not a place to carry 128 KiB per rider.
pub const MAX_COMMENT_CHARS: usize = 512;

/// Turns a Mumble comment into something showable on one line.
///
/// Tags are dropped, `<br>` and `</p>` become spaces rather than running two
/// sentences together, the five XML entities are decoded, and runs of
/// whitespace collapse. A tag that is never closed takes the rest of the string
/// with it, which is the safe direction: showing the inside of a malformed tag
/// would be showing markup.
pub fn strip_html(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_tag = false;
    let mut tag = String::new();
    /// Elements whose *contents* are not text either.
    ///
    /// **Qt writes a `<style>` block into every comment its editor produces**,
    /// so this is not a hypothetical defence against hostile markup but the
    /// difference between a roster line reading "back in ten" and one reading
    /// `p, li { white-space: pre-wrap; }`.
    const OPAQUE: [&str; 2] = ["script", "style"];
    let mut skipping: Option<&str> = None;
    let mut entity = String::new();
    let mut in_entity = false;

    for c in raw.chars() {
        match c {
            '<' => {
                in_tag = true;
                tag.clear();
                // A break in the markup is a break in the sentence.
                if out.chars().next_back().is_some_and(|c| !c.is_whitespace()) {
                    out.push(' ');
                }
            }
            '>' if in_tag => {
                in_tag = false;
                let name = tag
                    .trim_start_matches('/')
                    .split([' ', '\t', '\n', '/'])
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                match skipping {
                    Some(open) if tag.starts_with('/') && name == open => skipping = None,
                    Some(_) => {}
                    None => skipping = OPAQUE.iter().find(|o| **o == name).copied(),
                }
            }
            _ if in_tag => tag.push(c),
            _ if skipping.is_some() => {}
            '&' => {
                in_entity = true;
                entity.clear();
            }
            ';' if in_entity => {
                in_entity = false;
                match entity.as_str() {
                    "lt" => out.push('<'),
                    "gt" => out.push('>'),
                    "amp" => out.push('&'),
                    "quot" => out.push('"'),
                    "apos" | "#39" => out.push('\''),
                    "nbsp" => out.push(' '),
                    // Anything else is left as it was written: a rider's
                    // comment saying "R&D;" is not an entity and should not
                    // quietly lose characters.
                    other => {
                        out.push('&');
                        out.push_str(other);
                        out.push(';');
                    }
                }
                entity.clear();
            }
            _ if in_entity => {
                entity.push(c);
                // Entities are short. Anything longer was an ampersand in
                // ordinary text, so give it back and carry on.
                if entity.len() > 8 {
                    in_entity = false;
                    out.push('&');
                    out.push_str(&entity);
                    entity.clear();
                }
            }
            _ => out.push(c),
        }
    }
    if in_entity {
        out.push('&');
        out.push_str(&entity);
    }

    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(MAX_COMMENT_CHARS).collect()
}

/// What each rider's comment and picture look like, and what still has to be
/// asked for.
///
/// Keyed by session, so it is emptied by a reconnect for the same reason the
/// peer table is: session numbers are the server's and mean nothing afterwards.
#[derive(Debug, Default)]
pub struct Blobs {
    /// The hash of the comment we currently hold for a session.
    comment_hash: HashMap<u32, Vec<u8>>,
    /// The hash of the picture we currently hold.
    texture_hash: HashMap<u32, Vec<u8>>,
    /// Sessions whose comment body we still need.
    want_comment: HashSet<u32>,
    /// Sessions whose picture body we still need.
    want_texture: HashSet<u32>,
    /// The hash of the description we hold for a channel.
    channel_hash: HashMap<u32, Vec<u8>>,
    /// Channels whose description body we still need.
    want_channel: HashSet<u32>,
}

impl Blobs {
    /// Notes a hash the server sent for a comment, and whether its body is now
    /// worth asking for.
    ///
    /// An empty hash means *this rider has no comment*, which is a change worth
    /// recording and not something to request a body for.
    pub fn note_comment_hash(&mut self, session: u32, hash: &[u8]) -> bool {
        if hash.is_empty() {
            self.comment_hash.remove(&session);
            self.want_comment.remove(&session);
            return false;
        }
        if self.comment_hash.get(&session).is_some_and(|h| h == hash) {
            return false;
        }
        self.comment_hash.insert(session, hash.to_vec());
        self.want_comment.insert(session);
        true
    }

    /// As [`Blobs::note_comment_hash`], for a picture.
    pub fn note_texture_hash(&mut self, session: u32, hash: &[u8]) -> bool {
        if hash.is_empty() {
            self.texture_hash.remove(&session);
            self.want_texture.remove(&session);
            return false;
        }
        if self.texture_hash.get(&session).is_some_and(|h| h == hash) {
            return false;
        }
        self.texture_hash.insert(session, hash.to_vec());
        self.want_texture.insert(session);
        true
    }

    /// As [`Blobs::note_comment_hash`], for a channel's description.
    pub fn note_channel_hash(&mut self, channel: u32, hash: &[u8]) -> bool {
        if hash.is_empty() {
            self.channel_hash.remove(&channel);
            self.want_channel.remove(&channel);
            return false;
        }
        if self.channel_hash.get(&channel).is_some_and(|h| h == hash) {
            return false;
        }
        self.channel_hash.insert(channel, hash.to_vec());
        self.want_channel.insert(channel);
        true
    }

    /// Notes that a channel's description arrived.
    pub fn got_channel(&mut self, channel: u32) {
        self.want_channel.remove(&channel);
    }

    /// Forgets a channel that has been removed.
    pub fn forget_channel(&mut self, channel: u32) {
        self.channel_hash.remove(&channel);
        self.want_channel.remove(&channel);
    }

    /// Notes that a body arrived, so it is not asked for again.
    pub fn got_comment(&mut self, session: u32) {
        self.want_comment.remove(&session);
    }

    /// Notes that a picture arrived.
    pub fn got_texture(&mut self, session: u32) {
        self.want_texture.remove(&session);
    }

    /// The next batch to ask for: words first, pictures last.
    ///
    /// **Text before decoration, deliberately.** A comment is a line that might
    /// say where somebody is, and a channel description is what a rider reads
    /// to decide whether to join; a picture is decoration. On a link bad enough
    /// that only some of this arrives, the words are the half worth having.
    pub fn next_request(&self) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
        let take = |set: &HashSet<u32>, room: usize| {
            let mut v: Vec<u32> = set.iter().copied().collect();
            v.sort_unstable();
            v.truncate(room);
            v
        };

        let comments = take(&self.want_comment, BLOB_BATCH);
        let mut room = BLOB_BATCH.saturating_sub(comments.len());
        let channels = take(&self.want_channel, room);
        room = room.saturating_sub(channels.len());
        let textures = take(&self.want_texture, room);

        (comments, textures, channels)
    }

    /// Forgets a session that has left. The number goes back to the server and
    /// may come back attached to somebody else entirely.
    pub fn forget(&mut self, session: u32) {
        self.comment_hash.remove(&session);
        self.texture_hash.remove(&session);
        self.want_comment.remove(&session);
        self.want_texture.remove(&session);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_comes_out_as_a_line_of_text() {
        assert_eq!(
            strip_html("<p>back in <b>ten</b></p>"),
            "back in ten",
            "and not run together as \"back inten\""
        );
    }

    #[test]
    fn a_break_is_a_space_rather_than_a_join() {
        assert_eq!(
            strip_html("On the A9<br/>heading north"),
            "On the A9 heading north"
        );
    }

    #[test]
    fn entities_come_back_as_the_characters_they_stand_for() {
        assert_eq!(strip_html("Tom &amp; Jerry &lt;3"), "Tom & Jerry <3");
        assert_eq!(strip_html("&quot;quoted&quot;"), "\"quoted\"");
    }

    #[test]
    fn an_ampersand_in_ordinary_text_survives() {
        // "R&D" and "you & me" are not entities, and quietly eating the rest
        // of the sentence looking for a semicolon would be worse than leaving
        // the ampersand alone.
        assert_eq!(strip_html("R&D on tyres"), "R&D on tyres");
        assert_eq!(strip_html("fish & chips"), "fish & chips");
    }

    #[test]
    fn a_tag_that_is_never_closed_takes_the_rest_rather_than_showing_markup() {
        assert_eq!(strip_html("safe <b"), "safe");
    }

    #[test]
    fn a_style_block_is_not_a_comment() {
        // Qt's rich-text editor — the one Mumble's own client uses — puts one
        // of these at the top of every comment it writes. Stripping tags alone
        // leaves the CSS itself, and the roster reads as gibberish.
        let qt = "<style type=\"text/css\">p, li { white-space: pre-wrap; }</style>\
                  <p>back in ten</p>";
        assert_eq!(strip_html(qt), "back in ten");
    }

    #[test]
    fn a_script_body_is_not_shown_either() {
        // Never executed — nothing here renders HTML — but "alert(1)" in a
        // roster is still somebody else's text in a place it does not belong.
        assert_eq!(
            strip_html("safe <script>alert(1)</script> after"),
            "safe after"
        );
    }

    #[test]
    fn a_comment_is_not_allowed_to_be_a_hundred_kilobytes() {
        let long = "a".repeat(MAX_COMMENT_CHARS * 4);
        assert_eq!(strip_html(&long).chars().count(), MAX_COMMENT_CHARS);
    }

    #[test]
    fn multibyte_text_is_cut_by_characters_not_by_bytes() {
        let long = "я".repeat(MAX_COMMENT_CHARS * 2);
        let cut = strip_html(&long);
        assert_eq!(cut.chars().count(), MAX_COMMENT_CHARS);
        assert!(cut.ends_with('я'), "and never halfway through a character");
    }

    #[test]
    fn a_body_is_asked_for_once_per_hash() {
        let mut b = Blobs::default();
        assert!(b.note_comment_hash(3, b"abc"));
        assert_eq!(b.next_request().0, vec![3]);

        // Same hash again — the server re-sends these freely.
        assert!(!b.note_comment_hash(3, b"abc"));

        b.got_comment(3);
        assert!(b.next_request().0.is_empty());
        assert!(!b.note_comment_hash(3, b"abc"), "still the one we hold");

        // They changed it.
        assert!(b.note_comment_hash(3, b"def"));
        assert_eq!(b.next_request().0, vec![3]);
    }

    #[test]
    fn an_empty_hash_means_they_have_none() {
        let mut b = Blobs::default();
        b.note_comment_hash(3, b"abc");
        assert!(
            !b.note_comment_hash(3, b""),
            "clearing is not a body to fetch"
        );
        assert!(b.next_request().0.is_empty());
    }

    #[test]
    fn text_is_asked_for_before_pictures() {
        let mut b = Blobs::default();
        for s in 1..=12 {
            b.note_comment_hash(s, b"c");
            b.note_texture_hash(s, b"t");
        }
        let (comments, textures, _channels) = b.next_request();
        assert_eq!(comments.len(), BLOB_BATCH);
        assert!(
            textures.is_empty(),
            "on a bad link the words are the half worth having"
        );
    }

    #[test]
    fn pictures_get_the_room_the_comments_leave() {
        let mut b = Blobs::default();
        b.note_comment_hash(1, b"c");
        for s in 5..=9 {
            b.note_texture_hash(s, b"t");
        }
        let (comments, textures, _channels) = b.next_request();
        assert_eq!(comments, vec![1]);
        assert_eq!(textures, vec![5, 6, 7, 8, 9], "everything that fits");
    }

    #[test]
    fn somebody_leaving_takes_their_hashes_with_them() {
        let mut b = Blobs::default();
        b.note_comment_hash(3, b"abc");
        b.forget(3);
        assert!(b.next_request().0.is_empty());
        // The number is the server's to hand out again, so the same hash from
        // the next holder of it is news.
        assert!(b.note_comment_hash(3, b"abc"));
    }
}

#[cfg(test)]
mod channel_description_tests {
    use super::*;

    #[test]
    fn a_long_channel_description_is_asked_for_once_per_hash() {
        let mut b = Blobs::default();
        assert!(b.note_channel_hash(7, b"abc"));
        assert_eq!(b.next_request().2, vec![7]);
        assert!(!b.note_channel_hash(7, b"abc"), "the server re-sends these");
        b.got_channel(7);
        assert!(b.next_request().2.is_empty());
        assert!(b.note_channel_hash(7, b"def"), "somebody edited it");
    }

    #[test]
    fn a_description_that_was_cleared_is_not_fetched() {
        let mut b = Blobs::default();
        b.note_channel_hash(7, b"abc");
        assert!(!b.note_channel_hash(7, b""));
        assert!(b.next_request().2.is_empty());
    }

    #[test]
    fn words_come_before_decoration() {
        // One batch, and the pictures take what is left: a rider reads a
        // channel description to decide whether to join it, and looks at an
        // avatar never.
        let mut b = Blobs::default();
        for i in 1..=6 {
            b.note_comment_hash(i, b"c");
        }
        for i in 1..=6 {
            b.note_channel_hash(i, b"d");
        }
        for i in 1..=6 {
            b.note_texture_hash(i, b"t");
        }
        let (comments, textures, channels) = b.next_request();
        assert_eq!(comments.len(), 6);
        assert_eq!(channels.len(), 2, "what is left of the batch");
        assert!(textures.is_empty(), "and pictures wait for the next round");
    }

    #[test]
    fn a_removed_channel_takes_its_hash_with_it() {
        let mut b = Blobs::default();
        b.note_channel_hash(7, b"abc");
        b.forget_channel(7);
        assert!(b.next_request().2.is_empty());
        assert!(b.note_channel_hash(7, b"abc"), "the id may be reused");
    }
}
