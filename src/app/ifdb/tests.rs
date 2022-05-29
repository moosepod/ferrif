// Note: no separate test for store_save as this is tested as part of the various
// other save tests
// On windows, be sure to run with the testmode feature set:
// cargo test --features "testmode"
// otherwise console output will be disabled
use super::ifiction::{
    Bibilographic, Colophon, Contacts, Cover, CoverFormat, Forgiveness, Format, IFictionDate,
    Identification, Release, Resource, Story, Zcode,
};
#[allow(unused_imports)]
use super::{
    DbColor, DbFont, DbSave, DbTheme, IfdbConnection, LoadFileResult, Note, SaveType, ThemeType,
    WindowDetails, WindowType,
};
#[allow(unused_imports)]
use rusqlite::params;

#[allow(unused_imports)]
use std::path::PathBuf;

#[allow(dead_code)]
static INITIAL_DATA_IFID: &str = "ZCODE-1-200427-0000";

#[allow(dead_code)]
static INITIAL_STORY_DB_ID: u32 = 1;

#[cfg(test)]
fn setup_test_db() -> IfdbConnection {
    // Configures an in-memory test database with a single story pre-loaded
    match IfdbConnection::connect("file::memory:") {
        Ok(connection) => {
            connection.migrate().expect("Error migrating");

            // thanks to https://stackoverflow.com/questions/30003921/how-can-i-locate-resources-for-testing-with-cargo
            let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            d.push("resources");
            d.push("basic_2.z3");
            connection.import_file(
                d.as_os_str().to_str().expect("Error loading os str"),
                None,
                |x| println!("Loaded {:?}", x),
            );

            connection
        }
        Err(msg) => panic!("Error setting up db. {}", msg),
    }
}

#[test]
fn test_count_stories() {
    let connection = setup_test_db();

    // Assumes the setup has a single story loaded
    assert_eq!(1, connection.count_stories().expect("Failed with error."));
}

#[test]
fn test_get_story_id() {
    let connection = setup_test_db();

    // Assumes the setup has a single story with IFID ZCODE-1-200427-0000
    // Since it's the first story loaded, will have ID of 1

    assert_eq!(
        1,
        connection
            .get_story_id(INITIAL_DATA_IFID)
            .unwrap()
            .expect("Failed with error.")
    );

    assert!(connection
        .get_story_id("NOSUCHCODE")
        .expect("Failed with error")
        .is_none());
}

#[test]
fn test_get_story_data() {
    let connection = setup_test_db();

    // Assumes the setup has a single story with IFID ZCODE-1-200427-0000
    // Since it's the first story loaded, will have ID of 1
    // For simplicity, just check the length and not the raw bytes
    assert_eq!(
        2048,
        connection
            .get_story_data(1, INITIAL_DATA_IFID)
            .unwrap()
            .expect("Failed with error.")
            .len()
    );

    assert!(connection
        .get_story_data(2, INITIAL_DATA_IFID)
        .expect("Failed with error")
        .is_none());
    assert!(connection
        .get_story_data(1, "NOSUCHCODE")
        .expect("Failed with error")
        .is_none());
}

#[test]
fn test_get_story() {
    let connection = setup_test_db();
    // Assumes the setup has a single story with IFID ZCODE-1-200427-0000
    // Since it's the first story loaded, will have ID of 1
    // For simplicity, just check the length and not the raw bytes
    let story = connection
        .get_story(1)
        .unwrap()
        .expect("Failed with error.");

    assert_eq!(1, story.story_id);
    assert_eq!(0, story.time_played);
    assert!(story.last_played.is_none());

    let ifdb_story = story.story;

    assert_eq!(vec![INITIAL_DATA_IFID], ifdb_story.identification.ifids);
    assert_eq!(Format::ZCODE, ifdb_story.identification.format);

    assert_eq!("basic_2", ifdb_story.bibliographic.title);
    assert_eq!("Unknown", ifdb_story.bibliographic.author);
    assert!(ifdb_story.bibliographic.language.is_none());
    assert!(ifdb_story.bibliographic.headline.is_none());
    assert!(ifdb_story.bibliographic.first_published.is_none());
    assert!(ifdb_story.bibliographic.genre.is_none());
    assert!(ifdb_story.bibliographic.group.is_none());
    assert!(ifdb_story.bibliographic.series.is_none());
    assert!(ifdb_story.bibliographic.series_number.is_none());
    assert!(ifdb_story.bibliographic.forgiveness.is_none());
    assert!(ifdb_story.bibliographic.description.is_none());

    assert!(ifdb_story.resources.is_empty());
    assert!(ifdb_story.releases.is_empty());
    assert!(ifdb_story.contacts.is_none());
    assert!(ifdb_story.cover.is_none());
    assert!(ifdb_story.colophon.is_none());
    assert!(ifdb_story.zcode.is_none());
}

#[cfg(test)]
fn add_test_data_for_story(connection: &IfdbConnection, story_id: u32, ifid: &str) {
    let mut save = create_full_save(SaveType::Normal);
    save.ifid = ifid.to_string();
    connection.store_save(&save, false).expect("Error saving");

    connection
        .add_clue(
            story_id,
            String::from("test 1"),
            String::from("test 2"),
            String::from("test 3"),
        )
        .expect("Error storing clue");

    connection
        .save_note(Note {
            dbid: 0,
            story_id: story_id as i64,
            room_id: 17,
            notes: String::from("Test notes"),
            room_name: Some(String::from("Test room")),
            done: false,
        })
        .expect("Error saving note");

    connection
        .get_or_create_session(ifid.to_string())
        .expect("Error creating session");

    let mut details = WindowDetails {
        dbid: 0,
        story_id: story_id as i64,
        window_type: WindowType::Main,
        x: 1f64,
        y: 2f64,
        width: 3f64,
        height: 4f64,
        open: true,
    };
    connection
        .store_window_details(&mut details)
        .expect("Error storing");
}

#[cfg(test)]
fn check_no_data_for_story(connection: &IfdbConnection, story_id: u32) {
    assert!(connection
        .get_story(story_id)
        .expect("Failed with error")
        .is_none());
    assert_eq!(
        0,
        sql_count(
            connection,
            "SELECT count(*) FROM window_details WHERE story_id=? ",
            story_id,
        )
    );

    assert_eq!(
        0,
        sql_count(
            connection,
            "SELECT count(*) FROM story_resource WHERE story_id=? ",
            story_id,
        )
    );
    assert_eq!(
        0,
        sql_count(
            connection,
            "SELECT count(*) FROM story_release WHERE story_id=? ",
            story_id,
        )
    );
    assert_eq!(
        0,
        sql_count(
            connection,
            "SELECT count(*) FROM story_zcode WHERE story_id=? ",
            story_id,
        )
    );
    assert_eq!(
        0,
        sql_count(
            connection,
            "SELECT count(*) FROM story_ifid WHERE story_id=? ",
            story_id,
        )
    );
    assert_eq!(
        0,
        sql_count(
            connection,
            "SELECT COUNT (*) FROM clue_section WHERE  story_id = ?1",
            story_id,
        )
    );
    assert_eq!(
        0,
        sql_count(
            connection,
            "SELECT count(id) FROM saves WHERE ifid = (SELECT ifid FROM story_ifid WHERE story_id=?) ",
            story_id,
        )
    );
    assert_eq!(
        0,
        sql_count(
            connection,
            "SELECT count(id) FROM session WHERE ifid = (SELECT ifid FROM story_ifid WHERE story_id=?) ",
            story_id,
        )
    );
    assert_eq!(
        0,
        sql_count(
            connection,
            "SELECT COUNT (*) FROM clue WHERE subsection_id IN (SELECT id from clue_subsection WHERE subsection_id IN (SELECT id from clue_section WHERE story_id = ?))",
            story_id,
        )
    );
    assert_eq!(
        0,
        sql_count(
            connection,
            "SELECT COUNT (*) FROM clue_subsection WHERE section_id IN (SELECT id from clue_section WHERE story_id = ?)",
            story_id,
        )
    );

    assert_eq!(
        0,
        sql_count(
            connection,
            "SELECT COUNT(*) FROM notes WHERE story_id = ?1",
            story_id,
        )
    );
    assert_eq!(
        0,
        sql_count(
            connection,
            "SELECT COUNT(*) FROM map_room WHERE story_id = ?1",
            story_id,
        )
    );
}

#[cfg(test)]
fn check_data_for_story(connection: &IfdbConnection, story_id: u32) {
    assert!(connection
        .get_story(story_id)
        .expect("Failed with error")
        .is_some());

    assert_eq!(
        1,
        sql_count(
            connection,
            "SELECT count(*) FROM window_details WHERE story_id=? ",
            story_id,
        )
    );

    assert_eq!(
        1,
        sql_count(
            connection,
            "SELECT count(*) FROM story_resource WHERE story_id=? ",
            story_id,
        )
    );
    assert_eq!(
        1,
        sql_count(
            connection,
            "SELECT count(*) FROM story_release WHERE story_id=? ",
            story_id,
        )
    );
    assert_eq!(
        1,
        sql_count(
            connection,
            "SELECT count(*) FROM story_zcode WHERE story_id=? ",
            story_id,
        )
    );
    assert_eq!(
        1,
        sql_count(
            connection,
            "SELECT count(*) FROM story_ifid WHERE story_id=? ",
            story_id,
        )
    );

    assert_eq!(
        1,
        sql_count(
            connection,
            "SELECT count(id) FROM saves WHERE ifid = (SELECT ifid FROM story_ifid WHERE story_id=?) ",
            story_id,
        )
    );

    assert_eq!(
        1,
        sql_count(
            connection,
            "SELECT count(id) FROM session WHERE ifid = (SELECT ifid FROM story_ifid WHERE story_id=?) ",
            story_id,
        )
    );

    assert_eq!(
        1,
        sql_count(
            connection,
            "SELECT COUNT (*) FROM clue_section WHERE  story_id = ?1",
            story_id,
        )
    );
    assert_eq!(
        1,
        sql_count(
            connection,
            "SELECT COUNT (*) FROM clue WHERE subsection_id IN (SELECT id from clue_subsection WHERE subsection_id IN (SELECT id from clue_section WHERE story_id = ?))",
            story_id,
        )
    );
    assert_eq!(
        1,
        sql_count(
            connection,
            "SELECT COUNT (*) FROM clue_subsection WHERE section_id IN (SELECT id from clue_section WHERE story_id = ?)",
            story_id,
        )
    );

    assert_eq!(
        1,
        sql_count(
            connection,
            "SELECT COUNT(*) FROM notes WHERE story_id = ?1",
            story_id,
        )
    );

    assert_eq!(
        1,
        sql_count(
            connection,
            "SELECT COUNT(*) FROM map_room WHERE story_id = ?1",
            story_id,
        )
    );
}

#[cfg(test)]
fn sql_count(connection: &IfdbConnection, sql: &str, story_id: u32) -> u32 {
    let result = || -> Result<u32, rusqlite::Error> {
        let mut statement = connection.connection.prepare(sql)?;

        let mut query = statement.query(params![story_id])?;

        if let Some(row) = query.next()? {
            Ok(row.get(0)?)
        } else {
            Ok(0)
        }
    }();

    result.expect("SQL error")
}

#[test]
fn test_delete_story() {
    let connection = setup_test_db();
    // Setup pre-save data and validate the counts
    assert!(connection.create_story(full_story("ZCODE-12345")).is_ok());
    let first_story_id = 2;
    let first_ifid = "ZCODE-12345";
    assert!(connection.create_story(full_story("ZCODE-55555")).is_ok());
    let second_story_id = 3;
    let second_ifid = "ZCODE-55555";

    add_test_data_for_story(&connection, first_story_id, first_ifid);
    add_test_data_for_story(&connection, second_story_id, second_ifid);
    check_data_for_story(&connection, first_story_id);
    check_data_for_story(&connection, second_story_id);

    // Delete
    connection
        .delete_story(first_story_id)
        .expect("Failed with error");

    // Validate post-save
    check_no_data_for_story(&connection, first_story_id);
    check_data_for_story(&connection, second_story_id);
}

#[test]
fn test_fetch_ifids_for_story() {
    let connection = setup_test_db();

    // Assumes the setup has a single story with IFID ZCODE-1-200427-0000
    // Since it's the first story loaded, will have ID of 1
    // For simplicity, just check the length and not the raw bytes
    assert_eq!(
        vec![INITIAL_DATA_IFID],
        connection
            .fetch_ifids_for_story(1, true)
            .expect("Failed with error.")
    );

    assert!(connection
        .fetch_ifids_for_story(2, true)
        .expect("Failed with error")
        .is_empty());
}

#[test]
fn test_fetch_story_summaries() {
    let connection = setup_test_db();

    let summaries = connection
        .fetch_story_summaries(true, None)
        .expect("Failed with error.");
    assert_eq!(1, summaries.len());
    let story = &summaries[0];
    assert_eq!(1, story.story_id);
    assert_eq!(INITIAL_DATA_IFID, story.ifid);
    assert_eq!("basic_2", story.title);
    assert_eq!(0, story.time_played);
    assert!(story.last_played.is_none());

    // Test with search
    assert_eq!(
        1,
        connection
            .fetch_story_summaries(true, Some("basic"))
            .expect("Failed with error.")
            .len()
    );
    assert_eq!(
        1,
        connection
            .fetch_story_summaries(true, Some("Basic"))
            .expect("Failed with error.")
            .len()
    );
    assert_eq!(
        1,
        connection
            .fetch_story_summaries(true, Some("asic"))
            .expect("Failed with error.")
            .len()
    );

    assert_eq!(
        0,
        connection
            .fetch_story_summaries(true, Some("nope"))
            .expect("Failed with error.")
            .len()
    );
}

#[test]
fn test_get_story_summary_by_id() {
    let connection = setup_test_db();

    let story = connection
        .get_story_summary_by_id(1)
        .unwrap()
        .expect("Failed with error.");
    assert_eq!(1, story.story_id);
    assert_eq!(INITIAL_DATA_IFID, story.ifid);
    assert_eq!("basic_2", story.title);
    assert_eq!(0, story.time_played);
    assert!(story.last_played.is_none());

    assert!(connection
        .get_story_summary_by_id(2)
        .expect("Failed with error")
        .is_none());
}

#[test]
fn test_get_story_summary_by_ifid() {
    let connection = setup_test_db();

    let story = connection
        .get_story_summary_by_ifid(INITIAL_DATA_IFID)
        .unwrap()
        .expect("Failed with error.");
    assert_eq!(1, story.story_id);
    assert_eq!(INITIAL_DATA_IFID, story.ifid);
    assert_eq!("basic_2", story.title);
    assert_eq!(0, story.time_played);
    assert!(story.last_played.is_none());

    assert!(connection
        .get_story_summary_by_id(2)
        .expect("Failed with error")
        .is_none());
}

#[test]
fn test_get_story_id_for_ifid() {
    let connection = setup_test_db();

    assert_eq!(
        1,
        connection
            .get_story_id_for_ifid(INITIAL_DATA_IFID, false)
            .unwrap()
            .expect("Failed with error.")
    );

    assert!(connection
        .get_story_summary_by_id(2)
        .expect("Failed with error")
        .is_none());
}

#[test]
fn test_create_story_simple() {
    // Ensure that a story with no optional values stores
    let connection = setup_test_db();

    let story = Story {
        identification: Identification {
            ifids: vec![String::from("ZCODE-12345")],
            format: Format::ZCODE,
        },
        bibliographic: Bibilographic {
            title: String::from("A Title"),
            author: String::from("An Author"),
            language: None,
            headline: None,
            first_published: None,
            genre: None,
            group: None,
            series: None,
            series_number: None,
            forgiveness: None,
            description: None,
        },
        resources: vec![],
        contacts: None,
        cover: None,
        releases: vec![],
        colophon: None,
        zcode: None,
    };

    assert!(connection.create_story(story).is_ok());

    let story = connection
        .get_story(2)
        .unwrap()
        .expect("Failed with error.");

    assert_eq!(2, story.story_id);
    assert_eq!(0, story.time_played);
    assert!(story.last_played.is_none());

    let ifdb_story = story.story;

    assert_eq!(vec!["ZCODE-12345"], ifdb_story.identification.ifids);
    assert_eq!(Format::ZCODE, ifdb_story.identification.format);

    assert_eq!("A Title", ifdb_story.bibliographic.title);
    assert_eq!("An Author", ifdb_story.bibliographic.author);
    assert!(ifdb_story.bibliographic.language.is_none());
    assert!(ifdb_story.bibliographic.headline.is_none());
    assert!(ifdb_story.bibliographic.first_published.is_none());
    assert!(ifdb_story.bibliographic.genre.is_none());
    assert!(ifdb_story.bibliographic.group.is_none());
    assert!(ifdb_story.bibliographic.series.is_none());
    assert!(ifdb_story.bibliographic.series_number.is_none());
    assert!(ifdb_story.bibliographic.forgiveness.is_none());
    assert!(ifdb_story.bibliographic.description.is_none());

    assert!(ifdb_story.resources.is_empty());
    assert!(ifdb_story.releases.is_empty());
    assert!(ifdb_story.contacts.is_none());
    assert!(ifdb_story.cover.is_none());
    assert!(ifdb_story.colophon.is_none());
    assert!(ifdb_story.zcode.is_none());
}

#[allow(dead_code)]
fn full_story(ifid: &str) -> Story {
    // Used by other tests to create a story with every field used
    Story {
        identification: Identification {
            ifids: vec![ifid.to_string()],
            format: Format::ZCODE,
        },
        bibliographic: Bibilographic {
            title: String::from("A Title"),
            author: String::from("An Author"),
            language: Some(String::from("en")),
            headline: Some(String::from("Test headline")),
            first_published: Some(IFictionDate::Year(2022)),
            genre: Some(String::from("A Genre")),
            group: Some(String::from("A Group")),
            series: Some(String::from("A Series")),
            series_number: Some(1),
            forgiveness: Some(Forgiveness::Cruel),
            description: Some(String::from("This is a description")),
        },
        resources: vec![Resource {
            leafname: String::from("A Leaf"),
            description: String::from("A description"),
        }],
        contacts: Some(Contacts {
            url: Some(String::from("http://www.moosepod.com")),
            author_email: Some(String::from("foo@example.com")),
        }),
        cover: Some(Cover {
            cover_format: CoverFormat::JPG,
            height: 2,
            width: 3,
            description: Some(String::from("ABCDEF")),
            cover_image: Some(vec![1, 2, 3]),
        }),
        releases: vec![Release {
            version: 1,
            release_date: IFictionDate::Year(2021),
            compiler: Some(String::from("Inform")),
            compiler_version: Some(String::from("6")),
        }],
        colophon: Some(Colophon {
            generator: String::from("A Generator"),
            generator_version: Some(String::from("A version")),
            originated: IFictionDate::Year(1999),
        }),
        zcode: Some(Zcode {
            version: Some(5),
            release: Some(String::from("Foo")),
            serial: Some(String::from("123345")),
            checksum: Some(String::from("654321")),
            compiler: Some(String::from("Inform 6")),
            cover_picture: Some(1),
        }),
    }
}

#[test]
fn test_create_story_full() {
    // Ensure that a story with no optional values stores
    let connection = setup_test_db();

    assert!(connection.create_story(full_story("ZCODE-12345")).is_ok());

    let story = connection
        .get_story(2)
        .unwrap()
        .expect("Failed with error.");

    assert_eq!(2, story.story_id);
    assert_eq!(0, story.time_played);
    assert!(story.last_played.is_none());

    let ifdb_story = story.story;

    assert_eq!(vec!["ZCODE-12345"], ifdb_story.identification.ifids);
    assert_eq!(Format::ZCODE, ifdb_story.identification.format);

    assert_eq!("A Title", ifdb_story.bibliographic.title);
    assert_eq!("An Author", ifdb_story.bibliographic.author);
    assert_eq!("en", ifdb_story.bibliographic.language.unwrap());
    assert_eq!("Test headline", ifdb_story.bibliographic.headline.unwrap());
    assert_eq!(
        IFictionDate::Year(2022),
        ifdb_story.bibliographic.first_published.unwrap()
    );
    assert_eq!("A Genre", ifdb_story.bibliographic.genre.unwrap());
    assert_eq!("A Group", ifdb_story.bibliographic.group.unwrap());
    assert_eq!("A Series", ifdb_story.bibliographic.series.unwrap());
    assert_eq!(1, ifdb_story.bibliographic.series_number.unwrap());
    assert_eq!(
        Forgiveness::Cruel,
        ifdb_story.bibliographic.forgiveness.unwrap()
    );
    assert_eq!(
        "This is a description",
        ifdb_story.bibliographic.description.unwrap()
    );

    assert_eq!(
        vec![Resource {
            leafname: String::from("A Leaf"),
            description: String::from("A description")
        }],
        ifdb_story.resources
    );
    assert_eq!(
        vec![Release {
            version: 1,
            release_date: IFictionDate::Year(2021),
            compiler: Some(String::from("Inform")),
            compiler_version: Some(String::from("6"))
        }],
        ifdb_story.releases
    );
    assert!(ifdb_story.cover.is_some());
    if let Some(cover) = ifdb_story.cover {
        assert_eq!(CoverFormat::JPG, cover.cover_format,);
        assert_eq!(2, cover.height);
        assert_eq!(3, cover.width);
        assert_eq!(String::from("ABCDEF"), cover.description.unwrap());
        assert!(cover.cover_image.is_none()); // Cover image store not supported
    }
    assert!(ifdb_story.contacts.is_some());
    if let Some(contacts) = ifdb_story.contacts {
        assert_eq!(Some(String::from("http://www.moosepod.com")), contacts.url);
        assert_eq!(Some(String::from("foo@example.com")), contacts.author_email);
    }

    assert!(ifdb_story.colophon.is_some());
    if let Some(colophon) = ifdb_story.colophon {
        assert_eq!(String::from("A Generator"), colophon.generator);
        assert_eq!(Some(String::from("A version")), colophon.generator_version);
        assert_eq!(IFictionDate::Year(1999), colophon.originated);
    }

    assert!(ifdb_story.zcode.is_some());
    if let Some(zcode) = ifdb_story.zcode {
        assert_eq!(5, zcode.version.unwrap());
        assert_eq!(String::from("Foo"), zcode.release.unwrap());
        assert_eq!(String::from("123345"), zcode.serial.unwrap());
        assert_eq!(String::from("654321"), zcode.checksum.unwrap());
        assert_eq!(String::from("Inform 6"), zcode.compiler.unwrap());
        assert_eq!(1, zcode.cover_picture.unwrap());
    }
}
#[test]
fn test_update_last_played_to_now() {
    let connection = setup_test_db();
    let story = connection
        .get_story(1)
        .unwrap()
        .expect("Failed with error.");

    assert!(story.last_played.is_none());

    assert!(connection.update_last_played_to_now(1).is_ok());

    let story = connection
        .get_story(1)
        .unwrap()
        .expect("Failed with error.");

    // Really should check it is actual time, not just some, but a bit
    // of a pain to test
    assert!(story.last_played.is_some());
}

#[test]
fn test_add_to_time_played() {
    let connection = setup_test_db();
    let story = connection
        .get_story(1)
        .unwrap()
        .expect("Failed with error.");

    assert_eq!(0, story.time_played);

    assert!(connection.add_to_time_played(1, 3).is_ok());

    let story = connection
        .get_story(1)
        .unwrap()
        .expect("Failed with error.");

    assert_eq!(3, story.time_played);
}

#[test]
fn test_update_story() {
    let connection = setup_test_db();

    assert!(connection.create_story(full_story("ZCODE-12345")).is_ok());

    let mut story = connection
        .get_story(2)
        .unwrap()
        .expect("Failed with error.");
    story.story.bibliographic.title = String::from("A Title 2");
    story.story.bibliographic.author = String::from("An Author 2");
    story.story.bibliographic.language = Some(String::from("fr"));
    story.story.bibliographic.headline = Some(String::from("Test headline 2"));
    story.story.bibliographic.first_published = Some(IFictionDate::Year(2122));
    story.story.bibliographic.genre = Some(String::from("A Genre 2"));
    story.story.bibliographic.group = Some(String::from("A Group 2"));
    story.story.bibliographic.series = Some(String::from("A Series 2"));
    story.story.bibliographic.series_number = Some(2);
    story.story.bibliographic.forgiveness = Some(Forgiveness::Tough);
    story.story.bibliographic.description = Some(String::from("This is a description 2"));

    story.story.resources = vec![Resource {
        leafname: String::from("A Leaf 2"),
        description: String::from("A description 2"),
    }];
    story.story.contacts = Some(Contacts {
        url: Some(String::from("http://www.moosepod2.com")),
        author_email: Some(String::from("foo2@example.com")),
    });
    story.story.cover = Some(Cover {
        cover_format: CoverFormat::PNG,
        height: 12,
        width: 13,
        description: Some(String::from("FED")),
        cover_image: Some(vec![3, 2, 1]),
    });
    story.story.releases = vec![Release {
        version: 2,
        release_date: IFictionDate::Year(2121),
        compiler: Some(String::from("Informy")),
        compiler_version: Some(String::from("5")),
    }];
    story.story.colophon = Some(Colophon {
        generator: String::from("A Generator 2"),
        generator_version: Some(String::from("A version 2")),
        originated: IFictionDate::Year(2999),
    });
    story.story.zcode = Some(Zcode {
        version: Some(1),
        release: Some(String::from("Foo 2")),
        serial: Some(String::from("523345")),
        checksum: Some(String::from("554321")),
        compiler: Some(String::from("Inform 7")),
        cover_picture: Some(2),
    });

    assert!(connection.update_story(story).is_ok());

    // Check it after changes
    let story = connection
        .get_story(2)
        .unwrap()
        .expect("Failed with error.");

    assert_eq!(2, story.story_id);
    assert_eq!(0, story.time_played);
    assert!(story.last_played.is_none());

    let ifdb_story = story.story;

    assert_eq!(vec!["ZCODE-12345"], ifdb_story.identification.ifids);
    assert_eq!(Format::ZCODE, ifdb_story.identification.format);

    assert_eq!("A Title 2", ifdb_story.bibliographic.title);
    assert_eq!("An Author 2", ifdb_story.bibliographic.author);
    assert_eq!("fr", ifdb_story.bibliographic.language.unwrap());
    assert_eq!(
        "Test headline 2",
        ifdb_story.bibliographic.headline.unwrap()
    );
    assert_eq!(
        IFictionDate::Year(2122),
        ifdb_story.bibliographic.first_published.unwrap()
    );
    assert_eq!("A Genre 2", ifdb_story.bibliographic.genre.unwrap());
    assert_eq!("A Group 2", ifdb_story.bibliographic.group.unwrap());
    assert_eq!("A Series 2", ifdb_story.bibliographic.series.unwrap());
    assert_eq!(2, ifdb_story.bibliographic.series_number.unwrap());
    assert_eq!(
        Forgiveness::Tough,
        ifdb_story.bibliographic.forgiveness.unwrap()
    );
    assert_eq!(
        "This is a description 2",
        ifdb_story.bibliographic.description.unwrap()
    );

    // Unclear these update?
    assert_eq!(
        vec![Resource {
            leafname: String::from("A Leaf 2"),
            description: String::from("A description 2")
        }],
        ifdb_story.resources
    );
    assert_eq!(
        vec![Release {
            version: 2,
            release_date: IFictionDate::Year(2121),
            compiler: Some(String::from("Informy")),
            compiler_version: Some(String::from("5"))
        }],
        ifdb_story.releases
    );
    assert!(ifdb_story.cover.is_some());
    if let Some(cover) = ifdb_story.cover {
        assert_eq!(CoverFormat::PNG, cover.cover_format,);
        assert_eq!(12, cover.height);
        assert_eq!(13, cover.width);
        assert_eq!(String::from("FED"), cover.description.unwrap());
        assert!(cover.cover_image.is_none()); // Cover image store not supported
    }
    assert!(ifdb_story.contacts.is_some());
    if let Some(contacts) = ifdb_story.contacts {
        assert_eq!(Some(String::from("http://www.moosepod2.com")), contacts.url);
        assert_eq!(
            Some(String::from("foo2@example.com")),
            contacts.author_email
        );
    }

    assert!(ifdb_story.colophon.is_some());
    if let Some(colophon) = ifdb_story.colophon {
        assert_eq!(String::from("A Generator 2"), colophon.generator);
        assert_eq!(
            Some(String::from("A version 2")),
            colophon.generator_version
        );
        assert_eq!(IFictionDate::Year(2999), colophon.originated);
    }

    assert!(ifdb_story.zcode.is_some());
    if let Some(zcode) = ifdb_story.zcode {
        assert_eq!(1, zcode.version.unwrap());
        assert_eq!(String::from("Foo 2"), zcode.release.unwrap());
        assert_eq!(String::from("523345"), zcode.serial.unwrap());
        assert_eq!(String::from("554321"), zcode.checksum.unwrap());
        assert_eq!(String::from("Inform 7"), zcode.compiler.unwrap());
        assert_eq!(2, zcode.cover_picture.unwrap());
    }
}

#[test]
fn test_fetch_ifids() {
    let connection = setup_test_db();
    assert!(connection.create_story(full_story("ZCODE-12345")).is_ok());

    let ifids = connection.fetch_ifids(None, true).unwrap();
    assert_eq!(1, ifids.len());
    assert_eq!(
        vec![String::from("ZCODE-1-200427-0000")],
        *ifids.get(&1).unwrap()
    );

    let ifids = connection.fetch_ifids(None, false).unwrap();
    assert_eq!(2, ifids.len());
    assert_eq!(
        vec![String::from("ZCODE-1-200427-0000")],
        *ifids.get(&1).unwrap()
    );
    assert_eq!(vec![String::from("ZCODE-12345")], *ifids.get(&2).unwrap());

    let ifids = connection.fetch_ifids(Some(1), false).unwrap();
    assert_eq!(1, ifids.len());
    assert_eq!(
        vec![String::from("ZCODE-1-200427-0000")],
        *ifids.get(&1).unwrap()
    );
}

#[test]
fn test_fetch_resources() {
    let connection = setup_test_db();

    let resources = connection.fetch_resources(None).unwrap();
    assert_eq!(0, resources.len());
    assert!(connection.create_story(full_story("ZCODE-12345")).is_ok());

    let mut story = connection
        .get_story(2)
        .unwrap()
        .expect("Failed with error.");

    story.story.resources = vec![Resource {
        leafname: String::from("A Leaf 2"),
        description: String::from("A description 2"),
    }];

    assert!(connection.update_story(story).is_ok());

    let resources = connection.fetch_resources(None).unwrap();
    assert_eq!(1, resources.len());
    assert_eq!(
        vec![Resource {
            leafname: String::from("A Leaf 2"),
            description: String::from("A description 2")
        }],
        *resources.get(&2).unwrap()
    );

    let resources = connection.fetch_resources(Some(1)).unwrap();
    assert_eq!(0, resources.len());
    let resources = connection.fetch_resources(Some(2)).unwrap();
    assert_eq!(1, resources.len());
    assert_eq!(
        vec![Resource {
            leafname: String::from("A Leaf 2"),
            description: String::from("A description 2")
        }],
        *resources.get(&2).unwrap()
    );
}

#[test]
fn test_fetch_releases() {
    let connection = setup_test_db();

    let releases = connection.fetch_releases(None).unwrap();
    assert_eq!(0, releases.len());
    assert!(connection.create_story(full_story("ZCODE-12345")).is_ok());

    let mut story = connection
        .get_story(2)
        .unwrap()
        .expect("Failed with error.");

    story.story.releases = vec![Release {
        version: 2,
        release_date: IFictionDate::Year(2121),
        compiler: Some(String::from("Informy")),
        compiler_version: Some(String::from("5")),
    }];

    assert!(connection.update_story(story).is_ok());

    let releases = connection.fetch_releases(None).unwrap();
    assert_eq!(1, releases.len());
    assert_eq!(
        vec![Release {
            version: 2,
            release_date: IFictionDate::Year(2121),
            compiler: Some(String::from("Informy")),
            compiler_version: Some(String::from("5")),
        }],
        *releases.get(&2).unwrap()
    );

    let releases = connection.fetch_releases(Some(1)).unwrap();
    assert_eq!(0, releases.len());
    let releases = connection.fetch_releases(Some(2)).unwrap();
    assert_eq!(1, releases.len());
    assert_eq!(
        vec![Release {
            version: 2,
            release_date: IFictionDate::Year(2121),
            compiler: Some(String::from("Informy")),
            compiler_version: Some(String::from("5")),
        }],
        *releases.get(&2).unwrap()
    );
}

#[test]
fn test_fetch_zcode() {
    let connection = setup_test_db();

    let zcode = connection.fetch_zcode(None).unwrap();
    assert_eq!(0, zcode.len());
    assert!(connection.create_story(full_story("ZCODE-12345")).is_ok());

    let mut story = connection
        .get_story(2)
        .unwrap()
        .expect("Failed with error.");

    story.story.zcode = Some(Zcode {
        version: Some(1),
        release: Some(String::from("Foo 2")),
        serial: Some(String::from("523345")),
        checksum: Some(String::from("554321")),
        compiler: Some(String::from("Inform 7")),
        cover_picture: Some(2),
    });

    assert!(connection.update_story(story).is_ok());

    let zcode = connection.fetch_zcode(None).unwrap();
    assert_eq!(1, zcode.len());
    assert_eq!(
        Zcode {
            version: Some(1),
            release: Some(String::from("Foo 2")),
            serial: Some(String::from("523345")),
            checksum: Some(String::from("554321")),
            compiler: Some(String::from("Inform 7")),
            cover_picture: Some(2),
        },
        *zcode.get(&2).unwrap()
    );

    let zcode = connection.fetch_zcode(Some(1)).unwrap();
    assert_eq!(0, zcode.len());
    let zcode = connection.fetch_zcode(Some(2)).unwrap();
    assert_eq!(1, zcode.len());
    assert_eq!(
        Zcode {
            version: Some(1),
            release: Some(String::from("Foo 2")),
            serial: Some(String::from("523345")),
            checksum: Some(String::from("554321")),
            compiler: Some(String::from("Inform 7")),
            cover_picture: Some(2),
        },
        *zcode.get(&2).unwrap()
    );
}

#[test]
fn test_store_cover_image() {
    // Just a smoketest as there's no code yet to pull cover images out of the database
    let connection = setup_test_db();

    connection
        .store_cover_image(INITIAL_DATA_IFID, vec![0, 1, 2])
        .expect("Error saving");
}

#[test]
fn test_add_story_data() {
    let connection = setup_test_db();

    // Existing story
    connection
        .add_story_data(INITIAL_DATA_IFID, vec![0, 1, 2], "test")
        .expect("Error saving");
    assert_eq!(
        vec![0, 1, 2],
        connection
            .get_story_data(INITIAL_STORY_DB_ID, INITIAL_DATA_IFID)
            .expect("Error loading")
            .unwrap()
    );

    // Auto-create story
    let new_ifid = "12345";
    connection
        .add_story_data(new_ifid, vec![0, 1, 2, 3], "test 2")
        .expect("Error saving");
    assert_eq!(
        vec![0, 1, 2, 3],
        connection
            .get_story_data(2, new_ifid)
            .expect("Error loading")
            .unwrap()
    );
}

#[cfg(test)]
fn create_simple_save(save_type: SaveType) -> DbSave {
    DbSave {
        dbid: 0,
        version: 2,
        ifid: INITIAL_DATA_IFID.to_string(),
        name: String::from("test"),
        saved_when: String::new(),
        data: vec![1, 2, 3],
        save_type,
        pc: 0,
        parent_id: 0,
        room_id: 0,
        next_pc: None,
        text_buffer_address: None,
        parse_buffer_address: None,
        left_status: None,
        right_status: None,
        latest_text: None,
    }
}

#[cfg(test)]
fn create_full_save(save_type: SaveType) -> DbSave {
    DbSave {
        dbid: 0,
        version: 2,
        ifid: INITIAL_DATA_IFID.to_string(),
        name: String::from("test 2"),
        saved_when: String::new(),
        data: vec![1, 2, 3],
        save_type,
        pc: 123,
        parent_id: 1,
        room_id: 2,
        next_pc: Some(0x1234),
        text_buffer_address: Some(0x2345),
        parse_buffer_address: Some(0x3456),
        left_status: Some(String::from("Left Status")),
        right_status: Some(String::from("Right Status")),
        latest_text: Some(String::from("Latest Text")),
    }
}

#[test]
fn test_count_saves() {
    let connection = setup_test_db();
    assert_eq!(0, connection.count_saves().expect("Error counting saves"));
    let save = create_simple_save(SaveType::Normal);

    connection.store_save(&save, false).expect("Error saving");
    assert_eq!(1, connection.count_saves().expect("Error counting saves"));
    let mut save = create_simple_save(SaveType::Autosave);
    save.name = String::from("another name");
    connection.store_save(&save, false).expect("Error saving");
    assert_eq!(2, connection.count_saves().expect("Error counting saves"));
}

#[test]
fn test_count_autosaves_for_story() {
    let connection = setup_test_db();
    assert_eq!(
        0,
        connection
            .count_autosaves_for_story(INITIAL_DATA_IFID.to_string())
            .expect("Error counting saves")
    );
    let save = create_simple_save(SaveType::Normal);

    connection.store_save(&save, false).expect("Error saving");
    assert_eq!(
        0,
        connection
            .count_autosaves_for_story(INITIAL_DATA_IFID.to_string())
            .expect("Error counting saves")
    );
    let mut save = create_simple_save(SaveType::Autosave);
    save.name = String::from("another name");
    connection.store_save(&save, false).expect("Error saving");
    assert_eq!(
        1,
        connection
            .count_autosaves_for_story(INITIAL_DATA_IFID.to_string())
            .expect("Error counting saves")
    );

    assert_eq!(
        0,
        connection
            .count_autosaves_for_story(String::from("asdfasdf"))
            .expect("Error counting saves")
    );
}

#[test]
fn test_get_save() {
    let connection = setup_test_db();
    let save = create_full_save(SaveType::Normal);
    connection.store_save(&save, false).expect("Error saving");
    let dbsave = connection
        .get_save(INITIAL_DATA_IFID.to_string(), save.name.clone())
        .expect("Error fetching save");
    assert!(dbsave.is_some());
    if let Some(dbsave) = dbsave {
        assert_eq!(save.name, dbsave.name);
        assert_eq!(vec![1, 2, 3], dbsave.data);
        assert_eq!(SaveType::Normal, dbsave.save_type);
        assert_eq!(123, dbsave.pc);
        assert_eq!(Some(0x2345), dbsave.text_buffer_address);
        assert_eq!(Some(0x3456), dbsave.parse_buffer_address);
        assert_eq!(Some(0x1234), dbsave.next_pc);
        assert_eq!(Some(String::from("Left Status")), dbsave.left_status);
        assert_eq!(Some(String::from("Right Status")), dbsave.right_status);
        assert_eq!(Some(String::from("Latest Text")), dbsave.latest_text);
    }
}

#[test]
fn test_get_save_by_id() {
    let connection = setup_test_db();
    let save = create_full_save(SaveType::Normal);
    let dbid = connection.store_save(&save, false).expect("Error saving");
    let dbsave = connection
        .get_save_by_id(INITIAL_DATA_IFID.to_string(), dbid)
        .expect("Error fetching save");
    assert!(dbsave.is_some());
    if let Some(dbsave) = dbsave {
        assert_eq!(save.name, dbsave.name);
        assert_eq!(vec![1, 2, 3], dbsave.data);
        assert_eq!(SaveType::Normal, dbsave.save_type);
        assert_eq!(123, dbsave.pc);
        assert_eq!(Some(0x2345), dbsave.text_buffer_address);
        assert_eq!(Some(0x3456), dbsave.parse_buffer_address);
        assert_eq!(Some(0x1234), dbsave.next_pc);
        assert_eq!(Some(String::from("Left Status")), dbsave.left_status);
        assert_eq!(Some(String::from("Right Status")), dbsave.right_status);
        assert_eq!(Some(String::from("Latest Text")), dbsave.latest_text);
    }
}

#[test]
fn test_fetch_saves_for_ifid() {
    let connection = setup_test_db();
    connection
        .store_save(&create_simple_save(SaveType::Autosave), false)
        .expect("Error saving");

    let save = create_full_save(SaveType::Normal);
    connection.store_save(&save, false).expect("Error saving");
    let dbsaves = connection
        .fetch_saves_for_ifid(INITIAL_DATA_IFID.to_string())
        .expect("Error fetching save");
    assert_eq!(2, dbsaves.len());
    let dbsave = &dbsaves[1];
    assert_eq!(save.name, dbsave.name);
    assert_eq!(vec![1, 2, 3], dbsave.data);
    assert_eq!(SaveType::Normal, dbsave.save_type);
    assert_eq!(123, dbsave.pc);
    assert_eq!(Some(0x2345), dbsave.text_buffer_address);
    assert_eq!(Some(0x3456), dbsave.parse_buffer_address);
    assert_eq!(Some(0x1234), dbsave.next_pc);
    assert_eq!(Some(String::from("Left Status")), dbsave.left_status);
    assert_eq!(Some(String::from("Right Status")), dbsave.right_status);
    assert_eq!(Some(String::from("Latest Text")), dbsave.latest_text);
}

#[test]
fn test_fetch_manual_saves_for_ifid() {
    let connection = setup_test_db();
    let save = create_full_save(SaveType::Normal);
    connection.store_save(&save, false).expect("Error saving");
    connection
        .store_save(&create_simple_save(SaveType::Autosave), false)
        .expect("Error saving");
    let dbsaves = connection
        .fetch_manual_saves_for_ifid(INITIAL_DATA_IFID.to_string())
        .expect("Error fetching save");
    assert_eq!(1, dbsaves.len());
    let dbsave = &dbsaves[0];
    assert_eq!(save.name, dbsave.name);
    assert_eq!(vec![1, 2, 3], dbsave.data);
    assert_eq!(SaveType::Normal, dbsave.save_type);
    assert_eq!(123, dbsave.pc);
    assert_eq!(Some(0x2345), dbsave.text_buffer_address);
    assert_eq!(Some(0x3456), dbsave.parse_buffer_address);
    assert_eq!(Some(0x1234), dbsave.next_pc);
    assert_eq!(Some(String::from("Left Status")), dbsave.left_status);
    assert_eq!(Some(String::from("Right Status")), dbsave.right_status);
    assert_eq!(Some(String::from("Latest Text")), dbsave.latest_text);
}

#[test]
fn test_delete_autosaves_for_story() {
    let connection = setup_test_db();
    connection
        .store_save(&create_simple_save(SaveType::Autosave), false)
        .expect("Error saving");

    let save = create_full_save(SaveType::Normal);
    connection.store_save(&save, false).expect("Error saving");
    let dbsaves = connection
        .fetch_saves_for_ifid(INITIAL_DATA_IFID.to_string())
        .expect("Error fetching save");
    assert_eq!(2, dbsaves.len());

    connection
        .delete_autosaves_for_story(INITIAL_DATA_IFID.to_string())
        .expect("Error deleting");

    let dbsaves = connection
        .fetch_saves_for_ifid(INITIAL_DATA_IFID.to_string())
        .expect("Error fetching save");
    assert_eq!(1, dbsaves.len());

    assert_eq!(SaveType::Normal, dbsaves[0].save_type);
}

#[test]
fn test_get_or_create_session() {
    let connection = setup_test_db();

    let session = connection
        .get_or_create_session(INITIAL_DATA_IFID.to_string())
        .expect("Error creating session");

    assert!(!session.tools_open);
    assert!(!session.details_open);
    assert!(!session.debug_open);
    assert!(!session.transcript_active);
    assert!(!session.command_out_active);
    assert_eq!(
        String::from("transcript_ZCODE-1-200427-0000.log"),
        session.transcript_name
    );
    assert_eq!(
        String::from("commands_ZCODE-1-200427-0000.commands"),
        session.command_out_name
    );
    assert!(!session.clues_open);
    assert!(!session.notes_open);
    assert!(!session.map_open);
    assert_eq!(String::new(), session.last_clue_section);
    assert!(!session.saves_open);
}

#[test]
fn test_store_session() {
    let connection = setup_test_db();

    let mut session = connection
        .get_or_create_session(INITIAL_DATA_IFID.to_string())
        .expect("Error creating session");

    session.tools_open = true;
    session.details_open = true;
    session.debug_open = true;
    session.transcript_name = String::from("transcript2_ZCODE-1-200427-0000.log");
    session.transcript_active = true;
    session.command_out_name = String::from("commands2_ZCODE-1-200427-0000.commands");
    session.command_out_active = true;
    session.clues_open = true;
    session.notes_open = true;
    session.map_open = true;
    session.saves_open = true;
    session.last_clue_section = String::from("clues");

    connection
        .store_session(session)
        .expect("Error updating session");

    session = connection
        .get_or_create_session(INITIAL_DATA_IFID.to_string())
        .expect("Error creating session");

    assert!(session.tools_open);
    assert!(session.details_open);
    assert!(session.debug_open);
    assert!(session.transcript_active);
    assert!(session.command_out_active);
    assert_eq!(
        String::from("transcript2_ZCODE-1-200427-0000.log"),
        session.transcript_name
    );
    assert_eq!(
        String::from("commands2_ZCODE-1-200427-0000.commands"),
        session.command_out_name
    );
    assert!(session.clues_open);
    assert!(session.notes_open);
    assert!(session.map_open);
    assert_eq!(String::from("clues"), session.last_clue_section);
    assert!(session.saves_open);
}

//
// Clues
//
#[test]
fn test_count_clues() {
    let connection = setup_test_db();
    assert_eq!(0, connection.count_clues().expect("Error counting clues"));
    connection
        .add_clue(
            INITIAL_STORY_DB_ID,
            String::from("test 1"),
            String::from("test 2"),
            String::from("test 3"),
        )
        .expect("Error storing clue");
    assert_eq!(1, connection.count_clues().expect("Error counting clues"));
}

#[test]
fn test_reveal_and_hide_clue() {
    let connection = setup_test_db();
    connection
        .add_clue(
            1,
            String::from("test 1"),
            String::from("test 2"),
            String::from("test 3"),
        )
        .expect("Error storing clue");
    let clues = connection
        .get_clues_for_story(INITIAL_STORY_DB_ID)
        .expect("Error getting clues");
    let clue = clues[0].subsections[0].clues[0].clone();
    assert!(!clue.is_revealed);

    connection
        .reveal_clue(clue.dbid)
        .expect("Error revealing clue");
    let clues = connection
        .get_clues_for_story(INITIAL_STORY_DB_ID)
        .expect("Error getting clues");
    let clue = clues[0].subsections[0].clues[0].clone();
    assert!(clue.is_revealed);

    connection
        .hide_clue(clue.dbid)
        .expect("Error revealing clue");
    let clues = connection
        .get_clues_for_story(INITIAL_STORY_DB_ID)
        .expect("Error getting clues");
    let clue = clues[0].subsections[0].clues[0].clone();
    assert!(!clue.is_revealed);
}

#[test]
fn test_story_has_clues() {
    let connection = setup_test_db();
    assert!(!connection
        .story_has_clues(INITIAL_STORY_DB_ID)
        .expect("Error checking clues"));
    connection
        .add_clue(
            1,
            String::from("test 1"),
            String::from("test 2"),
            String::from("test 3"),
        )
        .expect("Error storing clue");
    assert!(connection
        .story_has_clues(INITIAL_STORY_DB_ID)
        .expect("Error checking clues"));
}

#[test]
fn test_add_clue_and_get_clues_for_story() {
    let connection = setup_test_db();
    connection
        .add_clue(
            1,
            String::from("test 1"),
            String::from("test 2"),
            String::from("test 3"),
        )
        .expect("Error storing clue");
    let clues = connection
        .get_clues_for_story(INITIAL_STORY_DB_ID)
        .expect("Error getting clues");
    assert_eq!(1, clues.len());
    assert_eq!(String::from("test 1"), clues[0].name);
    let subsections = &clues[0].subsections;
    assert_eq!(1, subsections.len());

    assert_eq!(String::from("test 2"), subsections[0].name);
    let clues = &subsections[0].clues;
    assert_eq!(1, clues.len());
    let clue = &clues[0];
    assert_eq!(String::from("test 3"), clue.text);
    assert!(!clue.is_revealed);
}

//
// Notes
//

#[test]
fn test_count_notes() {
    let connection = setup_test_db();
    assert_eq!(0, connection.count_notes().expect("Error counting notes"));
    let note = Note {
        dbid: 0,
        story_id: INITIAL_STORY_DB_ID as i64,
        room_id: 0,
        notes: String::from("Test notes"),
        room_name: None,
        done: false,
    };

    connection.save_note(note).expect("Error saving note");
    assert_eq!(1, connection.count_notes().expect("Error counting notes"));
}

#[test]
fn test_save_note_and_get_notes_for_story() {
    let connection = setup_test_db();
    let notes = connection
        .get_notes_for_story(INITIAL_STORY_DB_ID as i64, false)
        .expect("Error getting notes");
    assert_eq!(0, notes.len());

    let note = Note {
        dbid: 0,
        story_id: INITIAL_STORY_DB_ID as i64,
        room_id: 0,
        notes: String::from("Test notes"),
        room_name: None,
        done: false,
    };

    connection.save_note(note).expect("Error saving note");
    let notes = connection
        .get_notes_for_story(INITIAL_STORY_DB_ID as i64, false)
        .expect("Error getting notes");
    assert_eq!(1, notes.len());

    let note = &notes[0];
    assert_eq!(INITIAL_STORY_DB_ID as i64, note.story_id);
    assert_eq!(0, note.room_id);
    assert_eq!(String::from("Test notes"), note.notes);
    assert!(note.room_name.is_none());
    assert!(!note.done);

    let note = Note {
        dbid: 0,
        story_id: INITIAL_STORY_DB_ID as i64,
        room_id: 0,
        notes: String::from("Another note"),
        room_name: None,
        done: true,
    };

    connection.save_note(note).expect("Error saving note");

    let notes = connection
        .get_notes_for_story(INITIAL_STORY_DB_ID as i64, false)
        .expect("Error getting notes");
    assert_eq!(1, notes.len());

    let notes = connection
        .get_notes_for_story(INITIAL_STORY_DB_ID as i64, true)
        .expect("Error getting notes");
    assert_eq!(2, notes.len());
}

#[test]
fn test_set_note_done() {
    let connection = setup_test_db();

    let note = Note {
        dbid: 0,
        story_id: INITIAL_STORY_DB_ID as i64,
        room_id: 0,
        notes: String::from("Test notes"),
        room_name: None,
        done: false,
    };

    connection.save_note(note).expect("Error saving note");
    let notes = connection
        .get_notes_for_story(INITIAL_STORY_DB_ID as i64, true)
        .expect("Error getting notes");
    assert_eq!(1, notes.len());

    let note = &notes[0];
    assert!(!note.done);

    connection
        .set_note_done(note.dbid, true)
        .expect("Error setting note done");

    let notes = connection
        .get_notes_for_story(INITIAL_STORY_DB_ID as i64, true)
        .expect("Error getting notes");
    assert_eq!(1, notes.len());

    let note = &notes[0];
    assert!(note.done);

    connection
        .set_note_done(note.dbid, false)
        .expect("Error setting note done");
    let notes = connection
        .get_notes_for_story(INITIAL_STORY_DB_ID as i64, true)
        .expect("Error getting notes");
    assert_eq!(1, notes.len());

    let note = &notes[0];
    assert!(!note.done);
}

#[test]
fn test_set_note_notes() {
    let connection = setup_test_db();

    let note = Note {
        dbid: 0,
        story_id: INITIAL_STORY_DB_ID as i64,
        room_id: 0,
        notes: String::from("Test notes"),
        room_name: None,
        done: false,
    };

    connection.save_note(note).expect("Error saving note");
    let notes = connection
        .get_notes_for_story(INITIAL_STORY_DB_ID as i64, true)
        .expect("Error getting notes");
    assert_eq!(1, notes.len());

    let note = &notes[0];
    assert_eq!(String::from("Test notes"), note.notes);
    assert_eq!(0, note.room_id);

    connection
        .set_note_notes(note.dbid, String::from("Test notes 2"), 2)
        .expect("Error setting notes");
    let notes = connection
        .get_notes_for_story(INITIAL_STORY_DB_ID as i64, true)
        .expect("Error getting notes");
    assert_eq!(1, notes.len());

    let note = &notes[0];
    assert_eq!(String::from("Test notes 2"), note.notes);
    assert_eq!(2, note.room_id);
}

//
// Map
//
#[test]
fn test_get_rooms_for_story() {
    // Note rooms are not currently added directly, instead added via notes
    let connection = setup_test_db();
    let rooms = connection
        .get_rooms_for_story(INITIAL_STORY_DB_ID)
        .expect("Error fetching rooms");

    assert_eq!(0, rooms.len());

    let note = Note {
        dbid: 0,
        story_id: INITIAL_STORY_DB_ID as i64,
        room_id: 1,
        notes: String::from("Test notes"),
        room_name: Some(String::from("Test room")),
        done: false,
    };

    connection.save_note(note).expect("Error saving note");

    let rooms = connection
        .get_rooms_for_story(INITIAL_STORY_DB_ID)
        .expect("Error fetching rooms");
    assert_eq!(1, rooms.len());
    let room = &rooms[0];
    assert_eq!(String::from("Test room"), room.name);
    assert_eq!(1, room.room_id);
}

//
// Settings
//

#[test]
fn test_get_and_store_window_details() {
    let connection = setup_test_db();
    assert!(connection
        .get_window_details(INITIAL_STORY_DB_ID, WindowType::Main)
        .expect("Error getting details")
        .is_none());

    let mut details = WindowDetails {
        dbid: 0,
        story_id: INITIAL_STORY_DB_ID as i64,
        window_type: WindowType::Main,
        x: 1f64,
        y: 2f64,
        width: 3f64,
        height: 4f64,
        open: true,
    };
    connection
        .store_window_details(&mut details)
        .expect("Error storing");

    let details2 = connection
        .get_window_details(INITIAL_STORY_DB_ID, WindowType::Main)
        .expect("Error getting details")
        .unwrap();
    assert_eq!(details.story_id, details2.story_id);
    assert_eq!(details.window_type, details2.window_type);
    assert_eq!(details.x, details2.x);
    assert_eq!(details.y, details2.y);
    assert_eq!(details.width, details2.width);
    assert_eq!(details.height, details2.height);
    assert_eq!(details.open, details2.open);

    details.x = 4f64;
    details.y = 5f64;
    details.width = 6f64;
    details.height = 7f64;
    details.open = false;
    connection
        .store_window_details(&mut details)
        .expect("Error storing");
    let details2 = connection
        .get_window_details(INITIAL_STORY_DB_ID, WindowType::Main)
        .expect("Error getting details")
        .unwrap();
    assert_eq!(details.story_id, details2.story_id);
    assert_eq!(details.window_type, details2.window_type);
    assert_eq!(details.x, details2.x);
    assert_eq!(details.y, details2.y);
    assert_eq!(details.width, details2.width);
    assert_eq!(details.height, details2.height);
    assert_eq!(details.open, details2.open);
}

#[test]
fn test_get_and_store_current_story() {
    let connection = setup_test_db();
    assert!(connection
        .get_current_story()
        .expect("Error fetching current story")
        .is_none());
    connection
        .store_current_story(Some(1))
        .expect("Error storing current story");
    assert!(connection
        .get_current_story()
        .expect("Error fetching current story")
        .is_some());
    assert_eq!(
        1,
        connection
            .get_current_story()
            .expect("Error fetching current story")
            .unwrap()
    );
    connection
        .store_current_story(None)
        .expect("Error storing current story");
    assert!(connection
        .get_current_story()
        .expect("Error fetching current story")
        .is_none());
}

// Imports

#[allow(dead_code)]
fn test_data_path(filename: &str) -> String {
    // thanks to https://stackoverflow.com/questions/30003921/how-can-i-locate-resources-for-testing-with-cargo
    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    d.push("resources");
    d.push(filename);
    String::from(d.as_os_str().to_str().unwrap())
}

#[test]
fn test_import_save() {
    let connection = setup_test_db();
    assert_eq!(0, connection.count_saves().expect("Error counting saves"));
    if let Err(msg) = connection
        .import_save_from_file(INITIAL_DATA_IFID, test_data_path("valid_save.sav").as_str())
    {
        panic!("Error importing save: {}", msg);
    }
    assert_eq!(1, connection.count_saves().expect("Error counting saves"));

    // Importing again should thrown an error due
    if let Err(msg) = connection
        .import_save_from_file(INITIAL_DATA_IFID, test_data_path("valid_save.sav").as_str())
    {
        if msg != "A save with this name already exists" {
            panic!(
                "Should have thrown a duplicate name error, but error was {}",
                msg
            );
        }
    } else {
        panic!("Should have thrown a duplicate name error");
    }
}

#[test]
fn test_import_file_error() {
    let connection = setup_test_db();
    connection.import_file(test_data_path("nosuchfile").as_str(), None, |_| {
        // No file should load here, panic if it does
        panic!();
    });
}

#[test]
fn test_import_file_z3() {
    let connection = setup_test_db();
    assert_eq!(
        1,
        connection.count_stories().expect("Error counting stories")
    );
    connection.import_file(test_data_path("basic_3.z3").as_str(), None, |_| {});
    assert_eq!(
        2,
        connection.count_stories().expect("Error counting stories")
    );
}

#[test]
fn test_import_file_clues() {
    let connection = setup_test_db();
    assert_eq!(0, connection.count_clues().expect("Error counting clues"));
    connection.import_file(test_data_path("clues.json").as_str(), None, |_| {});

    assert_eq!(2, connection.count_clues().expect("Error counting clues"));
}

#[test]
fn test_import_file_zip() {
    let connection = setup_test_db();
    assert_eq!(0, connection.count_clues().expect("Error counting clues"));
    assert_eq!(
        1,
        connection.count_stories().expect("Error counting stories")
    );
    connection.import_file(test_data_path("zip.zip").as_str(), None, |_| {});
    assert_eq!(2, connection.count_clues().expect("Error counting clues"));
    assert_eq!(
        3,
        connection.count_stories().expect("Error counting stories")
    );
}

#[test]
fn test_import_file_cover() {
    let connection = setup_test_db();
    connection.import_file(
        test_data_path("ZCODE-1-200427-0000.png").as_str(),
        None,
        |r: LoadFileResult| match r {
            LoadFileResult::CoverImageSuccess(pathname, ifid) => {
                assert_eq!("ZCODE-1-200427-0000", pathname);
                assert_eq!("ZCODE-1-200427-0000", ifid);
            }
            _ => panic!("Expected success got {:?}", r),
        },
    );
}

#[test]
fn test_font_crud() {
    let connection = setup_test_db();

    // No fonts to start
    let fonts = connection.get_fonts();
    assert!(fonts.is_ok());
    assert_eq!(0, fonts.unwrap().len());

    // Check can add font
    assert!(connection.add_font("Test", vec![1, 2, 3], true).is_ok());

    // Font is there when added
    let fonts = connection.get_fonts();
    assert!(fonts.is_ok());

    let mut dbid = 0;
    if let Ok(fonts) = fonts {
        assert_eq!(1, fonts.len());

        if let Some(font) = fonts.get(0) {
            dbid = font.dbid;
            assert_eq!("Test", font.name);
            assert_eq!(vec![1, 2, 3], font.data);
            assert!(font.monospace);
        }
    }

    assert!(connection
        .update_font_metadata(DbFont {
            dbid,
            name: "Test 2".to_string(),
            data: vec![2, 3, 4],
            monospace: false
        })
        .is_ok());
    let fonts = connection.get_fonts();
    assert!(fonts.is_ok());
    if let Ok(fonts) = fonts {
        assert_eq!(1, fonts.len());

        if let Some(font) = fonts.get(0) {
            assert_eq!("Test 2", font.name);
            assert_eq!(vec![1, 2, 3], font.data); // Data unchanged
            assert!(!font.monospace);
        }
    }

    // Check delete

    assert!(connection.delete_font(dbid).is_ok());
    let fonts = connection.get_fonts();
    assert!(fonts.is_ok());
    assert_eq!(0, fonts.unwrap().len());
}
#[test]
fn test_get_and_store_theme() {
    let connection = setup_test_db();

    let theme = connection.get_theme("test");
    assert!(theme.is_ok());
    assert!(theme.expect("Error").is_none());

    // Test add with empty values
    let ui_theme = DbTheme {
        name: "ui".to_string(),
        theme_type: ThemeType::Custom,
        font_size: 12i64,
        font_id: None,
        background_color: None,
        text_color: None,
        stroke_color: None,
        secondary_background_color: None,
    };

    let result = connection.store_theme(ui_theme);
    assert!(result.is_ok());

    let theme = connection.get_theme("test");
    assert!(theme.is_ok());
    assert!(theme.expect("Error").is_none());

    let theme = connection.get_theme("ui").expect("Error").unwrap();
    assert_eq!("ui".to_string(), theme.name);
    assert_eq!(ThemeType::Custom, theme.theme_type);
    assert_eq!(12i64, theme.font_size);
    assert!(theme.font_id.is_none());
    assert!(theme.background_color.is_none());
    assert!(theme.text_color.is_none());
    assert!(theme.stroke_color.is_none());
    assert!(theme.secondary_background_color.is_none());

    // Test add with all values
    let story_theme = DbTheme {
        name: "story".to_string(),
        theme_type: ThemeType::Custom,
        font_size: 14i64,
        font_id: Some(11),
        background_color: Some(DbColor {
            r: 1,
            g: 2,
            b: 3,
            a: 4,
        }),
        text_color: Some(DbColor {
            r: 2,
            g: 3,
            b: 4,
            a: 5,
        }),
        stroke_color: Some(DbColor {
            r: 3,
            g: 4,
            b: 5,
            a: 6,
        }),
        secondary_background_color: Some(DbColor {
            r: 7,
            g: 8,
            b: 9,
            a: 10,
        }),
    };
    let result = connection.store_theme(story_theme);
    assert!(result.is_ok());

    let theme = connection.get_theme("story").expect("Error").unwrap();
    assert_eq!("story".to_string(), theme.name);
    assert_eq!(ThemeType::Custom, theme.theme_type);
    assert_eq!(14i64, theme.font_size);
    assert!(theme.font_id.is_some());
    assert_eq!(11, theme.font_id.unwrap());
    assert!(theme.background_color.is_some());
    assert_eq!(
        DbColor {
            r: 1,
            g: 2,
            b: 3,
            a: 4
        },
        theme.background_color.unwrap()
    );

    assert!(theme.text_color.is_some());
    assert_eq!(
        DbColor {
            r: 2,
            g: 3,
            b: 4,
            a: 5
        },
        theme.text_color.unwrap()
    );
    assert!(theme.stroke_color.is_some());
    assert_eq!(
        DbColor {
            r: 3,
            g: 4,
            b: 5,
            a: 6
        },
        theme.stroke_color.unwrap()
    );
    assert!(theme.secondary_background_color.is_some());
    assert_eq!(
        DbColor {
            r: 7,
            g: 8,
            b: 9,
            a: 10
        },
        theme.secondary_background_color.unwrap()
    );

    let story_theme = DbTheme {
        name: "story".to_string(),
        theme_type: ThemeType::Custom,
        font_size: 15i64,
        font_id: Some(12),
        background_color: Some(DbColor {
            r: 21,
            g: 22,
            b: 23,
            a: 24,
        }),
        text_color: Some(DbColor {
            r: 22,
            g: 23,
            b: 24,
            a: 25,
        }),
        stroke_color: Some(DbColor {
            r: 23,
            g: 24,
            b: 25,
            a: 26,
        }),
        secondary_background_color: Some(DbColor {
            r: 27,
            g: 28,
            b: 29,
            a: 210,
        }),
    };
    let result = connection.store_theme(story_theme);
    assert!(result.is_ok());

    let theme = connection.get_theme("story").expect("Error").unwrap();
    assert_eq!("story".to_string(), theme.name);
    assert_eq!(ThemeType::Custom, theme.theme_type);
    assert_eq!(15i64, theme.font_size);
    assert!(theme.font_id.is_some());
    assert_eq!(12, theme.font_id.unwrap());
    assert!(theme.background_color.is_some());
    assert_eq!(
        DbColor {
            r: 21,
            g: 22,
            b: 23,
            a: 24
        },
        theme.background_color.unwrap()
    );

    assert!(theme.text_color.is_some());
    assert_eq!(
        DbColor {
            r: 22,
            g: 23,
            b: 24,
            a: 25
        },
        theme.text_color.unwrap()
    );
    assert!(theme.stroke_color.is_some());
    assert_eq!(
        DbColor {
            r: 23,
            g: 24,
            b: 25,
            a: 26
        },
        theme.stroke_color.unwrap()
    );
    assert!(theme.secondary_background_color.is_some());
    assert_eq!(
        DbColor {
            r: 27,
            g: 28,
            b: 29,
            a: 210
        },
        theme.secondary_background_color.unwrap()
    );

    let story_theme = DbTheme {
        name: "story".to_string(),
        theme_type: ThemeType::Dark,
        font_size: 16i64,
        font_id: None,
        background_color: None,
        text_color: None,
        stroke_color: None,
        secondary_background_color: None,
    };
    let result = connection.store_theme(story_theme);
    assert!(result.is_ok());

    let theme = connection.get_theme("story").expect("Error").unwrap();
    assert_eq!("story".to_string(), theme.name);
    assert_eq!(ThemeType::Dark, theme.theme_type);
    assert_eq!(16i64, theme.font_size);
    assert!(theme.font_id.is_none());
    assert!(theme.background_color.is_none());

    assert!(theme.text_color.is_none());
    assert!(theme.secondary_background_color.is_none());
}
