///
/// Importer for IFiction files
/// See https://babel.ifarchive.org/babel_rev9.txt
///
extern crate xml;

use serde::{Serialize, Serializer};

use regex::Regex;
use std::io::Read;
use xml::reader::{EventReader, XmlEvent};

// No other formats are supported, and will be ignored
// See 5.5.2
// Clippy disabled: these values are from the spec and I prefer
// they match exactly
#[derive(PartialEq, Debug, Clone, Serialize)]
#[allow(clippy::upper_case_acronyms)]
pub enum Format {
    ZCODE,
}

pub fn convert_format_to_str(_: Format) -> String {
    "ZCODE".to_string()
}

// 5.5,. We ignore bafn code
#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct Identification {
    pub ifids: Vec<String>,
    pub format: Format,
}

// 5.6.5
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum IFictionDate {
    Year(u32),
    YearMonthDay(u32, u32, u32),
}

impl Serialize for IFictionDate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            IFictionDate::Year(y) => serializer.serialize_str(format!("{}", y).as_str()),
            IFictionDate::YearMonthDay(y, m, d) => {
                serializer.serialize_str(format!("{}-{:02}-{:02}", y, m, d).as_str())
            }
        }
    }
}

pub fn convert_ifictiondate_to_str(d: Option<IFictionDate>) -> Option<String> {
    match d {
        None => None,
        Some(e) => match e {
            IFictionDate::Year(y) => Some(format!("{}", y)),
            IFictionDate::YearMonthDay(y, m, d) => Some(format!("{}-{:02}-{:02}", y, m, d)),
        },
    }
}

#[allow(dead_code)]
pub fn convert_str_to_ifictiondate(s: Option<String>) -> Option<IFictionDate> {
    match s {
        None => None,
        Some(d) => match parse_ifictiondate(d.as_str()) {
            Ok(ifd) => Some(ifd),
            Err(_) => None,
        },
    }
}

// 5.6.10
#[derive(PartialEq, Debug, Copy, Clone, Serialize)]
pub enum Forgiveness {
    Merciful,
    Polite,
    Tough,
    Nasty,
    Cruel,
}

pub fn convert_forgiveness_to_str(d: Option<Forgiveness>) -> Option<String> {
    match d {
        None => None,
        Some(e) => match e {
            Forgiveness::Merciful => Some("Merciful".to_string()),
            Forgiveness::Polite => Some("Polite".to_string()),
            Forgiveness::Tough => Some("Tough".to_string()),
            Forgiveness::Nasty => Some("Nasty".to_string()),
            Forgiveness::Cruel => Some("Cruel".to_string()),
        },
    }
}

pub fn convert_str_to_forgiveness(s: Option<String>) -> Option<Forgiveness> {
    match s {
        None => None,
        Some(f) => match f.as_str() {
            "Merciful" => Some(Forgiveness::Merciful),
            "Polite" => Some(Forgiveness::Polite),
            "Tough" => Some(Forgiveness::Tough),
            "Nasty" => Some(Forgiveness::Nasty),
            "Cruel" => Some(Forgiveness::Cruel),
            _ => None,
        },
    }
}

// 5.
#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct Bibilographic {
    pub title: String,
    pub author: String,
    pub language: Option<String>,
    pub headline: Option<String>,
    pub first_published: Option<IFictionDate>,
    pub genre: Option<String>,
    pub group: Option<String>,
    pub series: Option<String>,
    pub series_number: Option<u32>,
    pub forgiveness: Option<Forgiveness>,
    pub description: Option<String>,
}

// 5.7
#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct Resource {
    pub leafname: String,
    pub description: String,
}

// 5.8
#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct Contacts {
    pub url: Option<String>,
    pub author_email: Option<String>,
}

// 5.9.1
// Clippy disabled: these values are from the spec and I prefer
// they match exactly
#[derive(PartialEq, Debug, Copy, Clone, Serialize)]
#[allow(clippy::upper_case_acronyms)]
pub enum CoverFormat {
    JPG,
    PNG,
}

pub fn convert_cover_format_to_str(cover_format: CoverFormat) -> &'static str {
    match cover_format {
        CoverFormat::JPG => "JPG",
        CoverFormat::PNG => "PNG",
    }
}

pub fn convert_str_to_cover_format(s: Option<String>) -> Option<CoverFormat> {
    match s {
        None => None,
        Some(f) => match f.as_str() {
            "JPG" => Some(CoverFormat::JPG),
            "PNG" => Some(CoverFormat::PNG),
            _ => None,
        },
    }
}

// 5.9
#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct Cover {
    pub cover_format: CoverFormat,
    pub height: u32,
    pub width: u32,
    pub description: Option<String>,
    pub cover_image: Option<Vec<u8>>,
}

// 5.11

#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct Release {
    pub version: u32,
    pub release_date: IFictionDate,
    pub compiler: Option<String>,
    pub compiler_version: Option<String>,
}

// 5.12
#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct Colophon {
    pub generator: String,
    pub generator_version: Option<String>,
    pub originated: IFictionDate,
}

#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct Zcode {
    pub version: Option<u32>,
    pub release: Option<String>,
    pub serial: Option<String>,
    pub checksum: Option<String>,
    pub compiler: Option<String>,
    pub cover_picture: Option<u32>,
}

#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct Story {
    pub identification: Identification,
    pub bibliographic: Bibilographic,
    pub resources: Vec<Resource>,
    pub contacts: Option<Contacts>,
    pub cover: Option<Cover>,
    pub releases: Vec<Release>,
    pub colophon: Option<Colophon>,
    pub zcode: Option<Zcode>,
}

fn remove_ifiction_prefix(s: String) -> String {
    s.replace("{http://babel.ifarchive.org/protocol/iFiction/}", "")
        .to_lowercase()
}

fn parse_ifictiondate(s: &str) -> Result<IFictionDate, &str> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"(\d\d\d\d)(-(\d\d)-(\d\d))?").unwrap();
    }
    if RE.is_match(s) {
        let captures = RE.captures(s).unwrap();
        if captures.get(2).is_none() {
            let year_string = captures.get(1).unwrap().as_str();
            Ok(IFictionDate::Year(year_string.parse::<u32>().unwrap()))
        } else {
            let year_string = captures.get(1).unwrap().as_str();
            let month_string = captures.get(3).unwrap().as_str();
            let day_string = captures.get(4).unwrap().as_str();
            Ok(IFictionDate::YearMonthDay(
                year_string.parse::<u32>().unwrap(),
                month_string.parse::<u32>().unwrap(),
                day_string.parse::<u32>().unwrap(),
            ))
        }
    } else {
        Err("Invalid date")
    }
}

fn cleanup_description(text: String) -> String {
    // Per 5.6.9, newlines/tabs/crs treated as spaces, multiple spaces as single space, and <br/> treated as newline
    lazy_static! {
        static ref RE: Regex = Regex::new(r"[\r\t\n ]+").unwrap();
    }

    let description = RE.replace_all(text.as_str(), " ").to_string();
    description.replace("<br/>", "\n")
}

#[derive(PartialEq, Debug, Copy, Clone)]
enum ParserState {
    Ifiction,
    Story,
    StoryError,
    Identification,
    Bibilographic,
    Auxiliary,
    Contacts,
    Cover,
    Release,
    Colophon,
    Zcode,
}

const STORY_TAG: &str = "story";
const IDENTIFICATION_TAG: &str = "identification";
const BIBLIOGRAPHIC_TAG: &str = "bibliographic";
const AUXILARY_TAG: &str = "auxiliary";
const CONTACTS_TAG: &str = "contacts";
const COVER_TAG: &str = "cover";
const RELEASE_TAG: &str = "release";
const COLOPHON_TAG: &str = "colophon";
const ZCODE_TAG: &str = "zcode";

fn parse_zcode<R: Read>(parser: &mut EventReader<R>) -> Result<Zcode, String> {
    let mut zcode = Zcode {
        version: None,
        release: None,
        serial: None,
        checksum: None,
        compiler: None,
        cover_picture: None,
    };
    let mut text = String::new();

    loop {
        match parser.next() {
            Ok(XmlEvent::Characters(s)) => {
                for c in s.chars() {
                    text.push(c);
                }
            }
            Ok(XmlEvent::EndElement { name, .. }) => {
                let match_name = remove_ifiction_prefix(name.to_string());
                match match_name.as_str() {
                    ZCODE_TAG => {
                        break;
                    }
                    "serial" => {
                        zcode.serial = Some(text.clone());
                    }

                    "checksum" => {
                        zcode.checksum = Some(text.clone());
                    }
                    "compiler" => {
                        zcode.compiler = Some(text.clone());
                    }
                    "release" => {
                        zcode.release = Some(text.clone());
                    }
                    "version" => match text.parse::<u32>() {
                        Ok(n) => zcode.version = Some(n),
                        Err(_) => {
                            return Err("Unable to parse version in zcode".to_string());
                        }
                    },
                    "coverpicture" => match text.parse::<u32>() {
                        Ok(n) => zcode.cover_picture = Some(n),
                        Err(_) => {
                            return Err("Unable to parse cover_picture in zcode".to_string());
                        }
                    },
                    _ => (),
                }
                text.clear();
            }
            Err(e) => return Err(e.to_string()),
            _ => (),
        }
    }

    Ok(zcode)
}

fn parse_cover<R: Read>(parser: &mut EventReader<R>) -> Result<Cover, String> {
    let mut cover_format: Option<CoverFormat> = None;
    let mut height: Option<u32> = None;
    let mut width: Option<u32> = None;
    let mut description: Option<String> = None;
    let mut text = String::new();
    loop {
        match parser.next() {
            Ok(XmlEvent::Characters(s)) => {
                for c in s.chars() {
                    text.push(c);
                }
            }
            Ok(XmlEvent::EndElement { name, .. }) => {
                let match_name = remove_ifiction_prefix(name.to_string());
                match match_name.as_str() {
                    COVER_TAG => {
                        break;
                    }
                    "format" => {
                        cover_format = match text.as_str() {
                            "jpg" => Some(CoverFormat::JPG),
                            "png" => Some(CoverFormat::PNG),
                            _ => None,
                        }
                    }
                    "description" => {
                        description = Some(cleanup_description(text.clone()));
                    }
                    "height" => match text.parse::<u32>() {
                        Ok(n) => height = Some(n),
                        Err(_) => {
                            return Err("Unable to parse height in cover".to_string());
                        }
                    },
                    "width" => match text.parse::<u32>() {
                        Ok(n) => width = Some(n),
                        Err(_) => {
                            return Err("Unable to parse height in cover".to_string());
                        }
                    },
                    _ => (),
                }
                text.clear();
            }
            Err(e) => return Err(e.to_string()),
            _ => (),
        }
    }

    if let Some(cover_format) = cover_format {
        if let Some(height) = height {
            if let Some(width) = width {
                Ok(Cover {
                    cover_format,
                    height,
                    width,
                    description,
                    cover_image: None,
                })
            } else {
                Err("Width not found in cover".to_string())
            }
        } else {
            Err("Height not found in cover".to_string())
        }
    } else {
        Err("Valid format not found in cover".to_string())
    }
}

fn parse_contacts<R: Read>(parser: &mut EventReader<R>) -> Result<Contacts, String> {
    let mut contact = Contacts {
        url: None,
        author_email: None,
    };
    let mut text = String::new();
    loop {
        match parser.next() {
            Ok(XmlEvent::Characters(s)) => {
                for c in s.chars() {
                    text.push(c);
                }
            }
            Ok(XmlEvent::EndElement { name, .. }) => {
                let match_name = remove_ifiction_prefix(name.to_string());
                match match_name.as_str() {
                    CONTACTS_TAG => {
                        break;
                    }
                    "url" => {
                        contact.url = Some(text.clone());
                    }
                    "authoremail" => {
                        contact.author_email = Some(text.clone());
                    }
                    _ => (),
                }
                text.clear();
            }
            Err(e) => return Err(e.to_string()),
            _ => (),
        }
    }

    Ok(contact)
}

fn parse_auxilary<R: Read>(parser: &mut EventReader<R>) -> Result<Resource, String> {
    let mut resource = Resource {
        leafname: String::new(),
        description: String::new(),
    };
    let mut text = String::new();
    loop {
        match parser.next() {
            Ok(XmlEvent::Characters(s)) => {
                for c in s.chars() {
                    text.push(c);
                }
            }
            Ok(XmlEvent::EndElement { name, .. }) => {
                let match_name = remove_ifiction_prefix(name.to_string());
                match match_name.as_str() {
                    AUXILARY_TAG => {
                        break;
                    }
                    "leafname" => {
                        resource.leafname = text.clone();
                    }
                    "description" => {
                        resource.description = text.clone();
                    }
                    _ => (),
                }
                text.clear();
            }
            Err(e) => return Err(e.to_string()),
            _ => (),
        }
    }

    Ok(resource)
}

fn parse_release<R: Read>(parser: &mut EventReader<R>) -> Result<Release, String> {
    let mut version = None;
    let mut release_date = None;
    let mut compiler = None;
    let mut compiler_version = None;
    let mut text = String::new();
    loop {
        match parser.next() {
            Ok(XmlEvent::Characters(s)) => {
                for c in s.chars() {
                    text.push(c);
                }
            }
            Ok(XmlEvent::EndElement { name, .. }) => {
                let match_name = remove_ifiction_prefix(name.to_string());
                match match_name.as_str() {
                    RELEASE_TAG => {
                        break;
                    }
                    "version" => match text.parse::<u32>() {
                        Ok(n) => version = Some(n),
                        Err(_msg) => {
                            return Err("Unable to parse version in release".to_string());
                        }
                    },
                    "releasedate" => match parse_ifictiondate(text.as_str()) {
                        Ok(date) => release_date = Some(date),
                        Err(msg) => return Err(msg.to_string()),
                    },
                    "compiler" => {
                        compiler = Some(text.clone());
                    }
                    "compilerversion" => {
                        compiler_version = Some(text.clone());
                    }
                    _ => (),
                }
                text.clear();
            }
            Err(e) => return Err(e.to_string()),
            _ => (),
        }
    }

    if let Some(version) = version {
        if let Some(release_date) = release_date {
            Ok(Release {
                version,
                release_date,
                compiler,
                compiler_version,
            })
        } else {
            Err("Release date not found in colophon".to_string())
        }
    } else {
        Err("Version not found in release".to_string())
    }
}

fn parse_colophon<R: Read>(parser: &mut EventReader<R>) -> Result<Colophon, String> {
    let mut generator = None;
    let mut generator_version = None;
    let mut originated = None;

    let mut text = String::new();
    loop {
        match parser.next() {
            Ok(XmlEvent::Characters(s)) => {
                for c in s.chars() {
                    text.push(c);
                }
            }
            Ok(XmlEvent::EndElement { name, .. }) => {
                let match_name = remove_ifiction_prefix(name.to_string());
                match match_name.as_str() {
                    COLOPHON_TAG => {
                        break;
                    }
                    "generator" => {
                        generator = Some(text.clone());
                    }
                    "generatorversion" => {
                        generator_version = Some(text.clone());
                    }
                    "originated" => match parse_ifictiondate(text.as_str()) {
                        Ok(date) => originated = Some(date),
                        Err(msg) => return Err(msg.to_string()),
                    },
                    _ => (),
                }
                text.clear();
            }
            Err(e) => return Err(e.to_string()),
            _ => (),
        }
    }
    if let Some(generator) = generator {
        if let Some(originated) = originated {
            Ok(Colophon {
                generator,
                generator_version,
                originated,
            })
        } else {
            Err("Originated not found in colophon".to_string())
        }
    } else {
        Err("Generator not found in colophon".to_string())
    }
}

fn parse_identification<R: Read>(parser: &mut EventReader<R>) -> Result<Identification, String> {
    let mut identification = Identification {
        ifids: vec![],
        format: Format::ZCODE,
    };
    let mut text = String::new();
    loop {
        match parser.next() {
            Ok(XmlEvent::Characters(s)) => {
                for c in s.chars() {
                    text.push(c);
                }
            }
            Ok(XmlEvent::EndElement { name, .. }) => {
                let match_name = remove_ifiction_prefix(name.to_string());
                match match_name.as_str() {
                    IDENTIFICATION_TAG => {
                        break;
                    }
                    "format" => {
                        if text != "zcode" {
                            return Err(format!("Unsupported format {}", text));
                        }
                    }
                    "ifid" => {
                        identification.ifids.push(text.clone());
                    }
                    _ => (),
                }
                text.clear();
            }
            Err(e) => return Err(e.to_string()),
            _ => (),
        }
    }

    Ok(identification)
}

fn parse_bibliographic<R: Read>(parser: &mut EventReader<R>) -> Result<Bibilographic, String> {
    let mut bibliographic = Bibilographic {
        title: String::new(),
        author: String::new(),
        language: None,
        headline: None,
        first_published: None,
        genre: None,
        group: None,
        series: None,
        series_number: None,
        forgiveness: None,
        description: None,
    };
    let mut text = String::new();
    loop {
        match parser.next() {
            Ok(XmlEvent::Characters(s)) => {
                for c in s.chars() {
                    text.push(c);
                }
            }
            Ok(XmlEvent::EndElement { name, .. }) => {
                let match_name = remove_ifiction_prefix(name.to_string());
                let mut clear_text = true;
                match match_name.as_str() {
                    BIBLIOGRAPHIC_TAG => {
                        break;
                    }
                    "title" => {
                        bibliographic.title = text.clone();
                    }
                    "author" => {
                        bibliographic.author = text.clone();
                    }
                    "language" => {
                        bibliographic.language = Some(text.clone());
                    }
                    "genre" => {
                        bibliographic.genre = Some(text.clone());
                    }
                    "group" => {
                        bibliographic.group = Some(text.clone());
                    }
                    "series" => {
                        bibliographic.series = Some(text.clone());
                    }
                    "headline" => {
                        bibliographic.headline = Some(text.clone());
                    }
                    "seriesnumber" => match text.parse::<u32>() {
                        Ok(n) => bibliographic.series_number = Some(n),
                        Err(_) => {
                            return Err("Unable to parse number in series".to_string());
                        }
                    },
                    "description" => {
                        bibliographic.description = Some(cleanup_description(text.clone()));
                    }
                    "forgiveness" => match text.to_lowercase().as_str() {
                        "merciful" => bibliographic.forgiveness = Some(Forgiveness::Merciful),
                        "polite" => bibliographic.forgiveness = Some(Forgiveness::Polite),
                        "tough" => bibliographic.forgiveness = Some(Forgiveness::Tough),
                        "nasty" => bibliographic.forgiveness = Some(Forgiveness::Nasty),
                        "cruel" => bibliographic.forgiveness = Some(Forgiveness::Cruel),
                        _ => return Err(format!("Invalid value for forgiveness: '{}'", text)),
                    },
                    "firstpublished" => match parse_ifictiondate(text.as_str()) {
                        Ok(date) => bibliographic.first_published = Some(date),
                        Err(msg) => return Err(msg.to_string()),
                    },
                    "br" => {
                        text.push('\n');
                        clear_text = false;
                    }
                    _ => (),
                }
                if clear_text {
                    text.clear();
                }
            }
            Err(e) => return Err(e.to_string()),
            _ => (),
        }
    }

    Ok(bibliographic)
}

struct TempStory {
    identification: Option<Identification>,
    bibiographic: Option<Bibilographic>,
    resources: Vec<Resource>,
    contacts: Option<Contacts>,
    cover: Option<Cover>,
    releases: Vec<Release>,
    colophon: Option<Colophon>,
    zcode: Option<Zcode>,
}

impl TempStory {
    pub fn new() -> TempStory {
        TempStory {
            identification: None,
            bibiographic: None,
            resources: vec![],
            contacts: None,
            cover: None,
            releases: vec![],
            colophon: None,
            zcode: None,
        }
    }

    pub fn to_story(&self) -> Result<Story, &str> {
        if self.identification.is_none() {
            Err("No valid identification on story")
        } else if self.bibiographic.is_none() {
            Err("No valid bibiographic on story")
        } else {
            Ok(Story {
                identification: self.identification.as_ref().unwrap().clone(),
                bibliographic: self.bibiographic.as_ref().unwrap().clone(),
                resources: self.resources.clone(),
                contacts: self.contacts.clone(),
                cover: self.cover.clone(),
                releases: self.releases.clone(),
                colophon: self.colophon.clone(),
                zcode: self.zcode.clone(),
            })
        }
    }
}

/// Takes the xml data in reader and converts it to Story objects
pub fn read_stories_from_xml(reader: impl Read) -> Result<Vec<Result<Story, String>>, String> {
    let mut parser = EventReader::new(reader);
    let mut state = ParserState::Ifiction;
    let mut stories = vec![];
    let mut current_story = TempStory::new();

    loop {
        match parser.next() {
            Ok(XmlEvent::StartElement { name, .. }) => {
                let match_name = remove_ifiction_prefix(name.to_string());
                match state {
                    ParserState::Ifiction => {
                        if match_name == STORY_TAG {
                            state = ParserState::Story;
                        }
                    }
                    ParserState::Story => {
                        state = match match_name.as_str() {
                            IDENTIFICATION_TAG => ParserState::Identification,
                            BIBLIOGRAPHIC_TAG => ParserState::Bibilographic,
                            AUXILARY_TAG => ParserState::Auxiliary,
                            CONTACTS_TAG => ParserState::Contacts,
                            COVER_TAG => ParserState::Cover,
                            ZCODE_TAG => ParserState::Zcode,
                            COLOPHON_TAG => ParserState::Colophon,
                            RELEASE_TAG => ParserState::Release,
                            _ => ParserState::Story,
                        };
                    }
                    ParserState::Colophon => match parse_colophon(&mut parser) {
                        Ok(colophon) => {
                            current_story.colophon = Some(colophon);
                            state = ParserState::Story;
                        }
                        Err(msg) => {
                            stories.push(Err(msg.to_string()));
                            state = ParserState::StoryError;
                        }
                    },
                    ParserState::Identification => match parse_identification(&mut parser) {
                        Ok(identification) => {
                            current_story.identification = Some(identification);
                            state = ParserState::Story;
                        }
                        Err(msg) => {
                            stories.push(Err(msg.to_string()));
                            state = ParserState::StoryError;
                        }
                    },
                    ParserState::Bibilographic => match parse_bibliographic(&mut parser) {
                        Ok(bibiographic) => {
                            current_story.bibiographic = Some(bibiographic);
                            state = ParserState::Story;
                        }
                        Err(msg) => {
                            stories.push(Err(msg.to_string()));
                            state = ParserState::StoryError;
                        }
                    },
                    ParserState::Cover => match parse_cover(&mut parser) {
                        Ok(cover) => {
                            current_story.cover = Some(cover);
                            state = ParserState::Story;
                        }
                        Err(msg) => {
                            stories.push(Err(msg.to_string()));
                            state = ParserState::StoryError;
                        }
                    },
                    ParserState::Zcode => match parse_zcode(&mut parser) {
                        Ok(zcode) => {
                            current_story.zcode = Some(zcode);
                            state = ParserState::Story;
                        }
                        Err(msg) => {
                            stories.push(Err(msg.to_string()));
                            state = ParserState::StoryError;
                        }
                    },
                    ParserState::Auxiliary => match parse_auxilary(&mut parser) {
                        Ok(resource) => {
                            current_story.resources.push(resource);
                            state = ParserState::Story;
                        }
                        Err(msg) => {
                            stories.push(Err(msg.to_string()));
                            state = ParserState::StoryError;
                        }
                    },
                    ParserState::Release => match parse_release(&mut parser) {
                        Ok(release) => {
                            current_story.releases.push(release);
                            state = ParserState::Story;
                        }
                        Err(msg) => {
                            stories.push(Err(msg.to_string()));
                            state = ParserState::StoryError;
                        }
                    },
                    ParserState::Contacts => match parse_contacts(&mut parser) {
                        Ok(contact) => {
                            current_story.contacts = Some(contact);
                            state = ParserState::Story;
                        }
                        Err(msg) => {
                            stories.push(Err(msg.to_string()));
                            state = ParserState::StoryError;
                        }
                    },
                    _ => (),
                }
            }
            Ok(XmlEvent::EndElement { name }) => match state {
                ParserState::StoryError => {
                    state = ParserState::Ifiction;
                }
                ParserState::Story => {
                    let match_name = remove_ifiction_prefix(name.to_string());
                    if match_name == STORY_TAG {
                        match current_story.to_story() {
                            Ok(story) => {
                                stories.push(Ok(story));
                            }
                            Err(msg) => {
                                stories.push(Err(msg.to_string()));
                            }
                        }
                        current_story = TempStory::new();
                        state = ParserState::Ifiction;
                    }
                }
                _ => (),
            },
            Err(e) => return Err(e.to_string()),
            Ok(XmlEvent::EndDocument { .. }) => {
                break;
            }
            _ => {}
        }
    }

    Ok(stories)
}
