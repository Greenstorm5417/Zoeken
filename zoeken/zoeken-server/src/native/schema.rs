//! Zoeken native search wire DTOs (`/api/v1/search`). Generated into TypeScript via `export-native-ts`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const NATIVE_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct NativeSearchRequest {
    pub q: String,
    #[serde(default = "default_pageno")]
    pub pageno: u32,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub safesearch: Option<u8>,
    #[serde(default)]
    pub categories: Option<String>,
    #[serde(default)]
    pub time_range: Option<String>,
    #[serde(default)]
    pub engines: Option<String>,
}

fn default_pageno() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct NativeSearchResponse {
    pub schema_version: u32,
    pub query: String,
    #[ts(type = "number")]
    pub number_of_results: u64,
    pub results: Vec<NativeResult>,
    pub answers: Vec<NativeAnswer>,
    pub corrections: Vec<NativeCorrection>,
    pub suggestions: Vec<NativeSuggestion>,
    pub infoboxes: Vec<NativeInfobox>,
    pub unresponsive_engines: Vec<NativeUnresponsiveEngine>,
    pub engine_data: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeResult {
    Main {
        url: String,
        title: String,
        content: String,
        engine: String,
        engines: Vec<String>,
        category: String,
        score: f64,
        positions: Vec<u32>,
        priority: String,
        thumbnail: String,
        iframe_src: String,
        favicon: String,
        pretty_url: String,
        published_date: Option<String>,
        length: String,
        author: String,
    },
    Image {
        url: String,
        title: String,
        content: String,
        engine: String,
        engines: Vec<String>,
        score: f64,
        positions: Vec<u32>,
        priority: String,
        img_src: String,
        thumbnail_src: String,
        resolution: String,
        img_format: String,
        source: String,
        filesize: String,
    },
    Paper {
        url: String,
        title: String,
        content: String,
        engine: String,
        engines: Vec<String>,
        score: f64,
        positions: Vec<u32>,
        priority: String,
        authors: Vec<String>,
        doi: String,
        journal: String,
        published_date: Option<String>,
        publisher: String,
        editor: String,
        volume: String,
        pages: String,
        number: String,
        #[serde(rename = "type")]
        #[ts(rename = "type")]
        type_: String,
        tags: Vec<String>,
        issn: Vec<String>,
        isbn: Vec<String>,
        pdf_url: String,
        html_url: String,
        comments: String,
    },
    Code {
        url: String,
        title: String,
        content: String,
        engine: String,
        engines: Vec<String>,
        score: f64,
        positions: Vec<u32>,
        priority: String,
        repository: String,
        filename: String,
        code_language: String,
        codelines: Vec<(u32, String)>,
        hl_lines: Vec<u32>,
    },
    File {
        url: String,
        title: String,
        content: String,
        engine: String,
        engines: Vec<String>,
        score: f64,
        positions: Vec<u32>,
        priority: String,
        filename: String,
        size: String,
        time: String,
        mimetype: String,
        #[serde(rename = "abstract")]
        #[ts(rename = "abstract")]
        abstract_: String,
        author: String,
        embedded: String,
        mtype: String,
        subtype: String,
        filesize: String,
        #[ts(type = "number | null")]
        seed: Option<i64>,
        #[ts(type = "number | null")]
        leech: Option<i64>,
        magnetlink: String,
    },
    KeyValue {
        url: String,
        title: String,
        content: String,
        engine: String,
        engines: Vec<String>,
        score: f64,
        positions: Vec<u32>,
        priority: String,
        caption: String,
        key_title: String,
        value_title: String,
        kvmap: Vec<(String, String)>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct NativeAnswer {
    pub answer: String,
    pub url: Option<String>,
    pub engine: String,
    pub interactive: Option<NativeInteractiveAnswer>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NativeInteractiveAnswer {
    Unit {
        amount: f64,
        from: String,
        to: String,
        result: f64,
        dimension: String,
    },
    Currency {
        amount: f64,
        from: String,
        to: String,
        result: f64,
        rate: f64,
    },
    Calculator {
        expression: String,
        result: f64,
    },
    Weather {
        place: String,
        description: String,
        temp_c: String,
        temp_f: String,
        feels_c: String,
        wind_kmph: String,
        wind_dir: String,
        humidity: String,
    },
    SelfInfo {
        kind: String,
        value: String,
    },
    Crypto {
        mode: String,
        algorithm: String,
        input: String,
    },
    Translate {
        source: String,
        target_lang: String,
        translated: String,
    },
    Dictionary {
        term: String,
        definitions: Vec<String>,
    },
    Wikipedia {
        title: String,
        extract: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        img_src: String,
        #[serde(default)]
        url: String,
        #[serde(default)]
        wikidata_id: String,
        #[serde(default)]
        attributes: Vec<NativeInfoboxAttribute>,
        #[serde(default)]
        related_topics: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NativeCorrection {
    pub correction: String,
    pub url: Option<String>,
    pub engine: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NativeSuggestion {
    pub suggestion: String,
    pub engine: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NativeInfoboxUrl {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NativeInfoboxImage {
    pub src: String,
    #[serde(default)]
    pub alt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NativeInfoboxAttribute {
    pub label: String,
    #[serde(default)]
    pub value: String,
    pub image: Option<NativeInfoboxImage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct NativeInfobox {
    pub infobox: String,
    pub id: Option<String>,
    pub content: String,
    pub img_src: Option<String>,
    pub urls: Vec<NativeInfoboxUrl>,
    pub attributes: Vec<NativeInfoboxAttribute>,
    pub related_topics: Vec<String>,
    pub engine: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NativeUnresponsiveEngine {
    pub engine: String,
    pub cause: String,
}
