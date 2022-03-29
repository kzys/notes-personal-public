use chrono::prelude::*;
use handlebars::{to_json, Handlebars};
use itertools::Itertools;
use pulldown_cmark::{html, Event, HeadingLevel::H1, Parser, Tag};
use serde::Serialize;
use serde_json::value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

mod hack;

#[derive(Serialize)]
struct Page {
    title: String,
    html: String,
    html_path: String,
    size: u64,
    changes: Vec<u64>,
    created_at: Option<String>,
    last_modified_at: Option<String>,
}

impl Page {
    fn created_at(&self) -> Option<DateTime<Utc>> {
        if self.changes.len() == 0 {
            None
        } else {
            Some(Utc.timestamp(self.changes[self.changes.len() - 1] as i64, 0))
        }
    }
    fn last_modified_at(&self) -> Option<DateTime<Utc>> {
        if self.changes.len() == 0 {
            None
        } else {
            Some(Utc.timestamp(self.changes[0] as i64, 0))
        }
    }
}

fn find_title<'a>(it: impl Iterator<Item = Event<'a>>) -> Option<String> {
    let mut heading = false;
    let mut title = None;
    it.for_each(|ev| match ev {
        Event::Start(Tag::Heading(H1, _, _)) => heading = true,
        Event::End(Tag::Heading(H1, _, _)) => heading = false,
        Event::Text(text) => {
            if heading {
                title = Some(text.to_string())
            }
        }
        _ => {}
    });
    title
}

fn find_files(from: &Path, ext: &str) -> hack::Result<Vec<PathBuf>> {
    let mut result: Vec<PathBuf> = vec![];
    let paths = fs::read_dir(from)?;
    for path in paths {
        let path = path?;
        let mut pb = PathBuf::from(&path.file_name());
        if from.as_os_str() != "." {
            pb = from.join(pb)
        }
        if let Ok(t) = path.file_type() {
            if t.is_dir() {
                result.append(&mut find_files(&pb, ext)?)
            }

            if let Some(e) = pb.extension() {
                if e == ext {
                    result.push(pb);
                }
            }
        }
    }
    Ok(result)
}

fn collect_pages(
    dir: &str,
    files: &std::collections::HashMap<String, Vec<u64>>,
) -> hack::Result<Vec<Page>> {
    let dir = Path::new(dir);
    let paths = find_files(dir, "md")?;
    let mut pages = Vec::new();
    for path in paths {
        let converted_path = path.to_str().unwrap().replace(".md", ".html");

        let p = if path.ends_with("README.md") {
            "index.html"
        } else {
            &converted_path
        };

        let html_path = if dir.as_os_str() != "." {
            PathBuf::from(p)
        } else {
            dir.join(p)
        };

        let content = fs::read_to_string(&path)?;
        let parser = Parser::new(&content);

        let (it1, it2) = parser.tee();
        let title = find_title(it1);

        let mut html = String::new();
        html::push_html(&mut html, it2);

        let size = fs::metadata(&path)?.len();
        let empty = vec![];
        let k = path.to_str().unwrap().to_string();
        let changes = files.get(&k).unwrap_or(&empty);

        let mut p = Page {
            title: title.unwrap_or_else(|| path.to_str().unwrap().to_string().clone()),
            html,
            html_path: html_path.to_string_lossy().to_string(),
            size,
            changes: changes.to_vec(),
            created_at: None,
            last_modified_at: None,
        };
        p.created_at = p.created_at().map(|x| x.to_string());
        p.last_modified_at = p.last_modified_at().map(|x| x.to_string());
        pages.push(p);
    }
    Ok(pages)
}

fn git_log() -> hack::Result<std::collections::HashMap<String, Vec<u64>>> {
    let git_log = Command::new("git")
        .args(["log", "--format=format:commit\t%H\t%ct", "--numstat"])
        .output();
    let stdout_vec = git_log?.stdout;
    let stdout = std::str::from_utf8(&stdout_vec)?;
    let lines = stdout.split("\n");

    let mut files = std::collections::HashMap::<String, Vec<u64>>::new();

    let mut dt: Option<u64> = None;
    for line in lines {
        let columns: Vec<&str> = line.split("\t").collect();
        if columns.len() > 2 && columns[0] == "commit" {
            dt = Some(u64::from_str(columns[2])?);
        } else if columns.len() > 2 {
            let mut key = columns[2].to_string();
            let rename: Vec<&str> = key.split(" => ").collect();
            if rename.len() == 2 {
                key = rename[1].to_string();
            }
            files.entry(key).or_insert(vec![]).push(dt.unwrap());
        } else {
            dt = None;
        }
    }

    Ok(files)
}

fn main() -> hack::Result<()> {
    let mut reg = Handlebars::new();
    let tp = fs::read_to_string("system/template.html")?;
    reg.register_template_string("tp", tp)?;

    fs::create_dir_all("build")?;

    let files = git_log()?;
    let pages: Vec<Page> = collect_pages(".", &files)?;

    let toc: Vec<&Page> = pages
        .iter()
        .sorted_by(|a, b| a.title.cmp(&b.title))
        .filter(|x| x.html_path != "index.html")
        .collect();

    for page in &pages {
        let mut data = value::Map::new();
        data.insert("title".to_string(), to_json(&page.title));
        data.insert("size".to_string(), to_json(page.size));
        data.insert("page".to_string(), to_json(&page));

        if page.html_path.ends_with("index.html") {
            data.insert("pages".to_string(), to_json(&toc));
        }

        let dest = Path::new("build").join(&page.html_path);
        if let Some(p) = dest.parent() {
            fs::create_dir_all(p)?;
        }
        let f = fs::File::create(dest)?;
        reg.render_to_write("tp", &data, f)?;
    }

    Ok(())
}
