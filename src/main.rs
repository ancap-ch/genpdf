#![recursion_limit = "1024"]
#[macro_use]
extern crate tera;
#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate lazy_static;
#[macro_use] extern crate log;
extern crate env_logger;
extern crate serde_yaml;
extern crate semver;
extern crate regex;
//extern crate walkdir;

mod errors {
    error_chain!{}
}
use errors::*;

//use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::fs::read_dir;
use std::path::Path;
use std::path::PathBuf;
//use std::io::prelude::*;
use tera::Tera;

use semver::Version;
use regex::Regex;
//use walkdir::WalkDir;
//use log::Level;

use std::collections::HashMap;
use std::collections::HashSet;

use std::process::Command;
//use std::ffi::OsStr;


type VS = Vec<String>;
type OVS = Option<Vec<String>>;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
struct InfoTranslation {
    language: String,
    is_translation: bool,
    this_project_url: Option<String>,
    fetch_translators: bool,
    fetch_reviwers: bool,
    fetch_progress: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
struct InfoPerson {
    identifier: String,
    rule: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
struct InfoResource {
    rule: Option<String>,
    content: Option<String>,
    websites: OVS,
    description: Option<String>,
    persons: OVS,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
struct InfoTarget {
    name: String,
    reset_footer_active: bool,
    reset_footer_depth: u8,
    clear_page_active: bool,
    clear_page_depth: u8,
    toc_depth: u8,
}


#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
struct Info {
    content_files: Vec<VS>,
    translation: InfoTranslation,
    titles: VS,
    discussions: Option<Vec<VS>>,
    more_infos: Option<Vec<VS>>,
    tags: OVS,
    tag_prefix: Option<String>,
    persons_id: Option<Vec<InfoPerson>>,
    resources: Option<Vec<InfoResource>>,
    targets: Vec<InfoTarget>,
    version: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct InfoPerson2 {
    name: String,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
struct Info2 {
    authors: Vec<InfoPerson2>,
    translators: Vec<InfoPerson2>,
    collaborators: Vec<InfoPerson2>,
    thanks: Vec<InfoPerson2>,
    reviewers: Vec<InfoPerson2>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Consts {
    min_ver: String,
    passages: u8,
    cover_nodes: Vec<String>,
    all_langs_from_dir: String,
    all_langs_to_dir: String,
    all_langs: Vec<Lang>,
}

lazy_static! {
    pub static ref TERA: Tera = {
        let mut tera = compile_templates!("templates/**/*");
        tera.autoescape_on(vec![".tex"]);
        //tera.register_filter("do_nothing", do_nothing_filter);
        tera
    };
    pub static ref RE_FORWARD_ARROW: Regex = 
        Regex::new("\\{->|\\{-&gt;").unwrap(); // some macros may output -> as {-&gt;
}


#[derive(Serialize, Deserialize, Debug, Clone)]
struct Lang {
    from_active: bool,
    to_active: bool,
    to_dir_name: String, // pt-BR
    set_lang: String, // brazil (xelatex)
    title: String, // Portuguese (Brazilian)
    from_url: Option<String>, // https://crowdin.com/project/ancap-ch/
    from_dir_name: Option<String>, // from_en
}

#[derive(Serialize, Deserialize, Debug)]
struct Defaults {
    info: Info,
    info2: Info2,
    target: String,

    all_langs: Vec<Lang>,
    def_lang: Lang,
    other_langs: Vec<Lang>,

    consts: Consts,
}

fn run() -> Result<()> {
    let ymlc = File::open("const.yml")
        .chain_err(|| "Failed to open the yml const file")?;
    let consts: Consts = serde_yaml::from_reader(ymlc)
        .chain_err(|| "Failed to parse the yml const file contents")?;
    let min_ver = Version::parse(&consts.min_ver)
        .chain_err(|| format!("Failed to parse the consts version ({})", &consts.min_ver))?;


    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
    struct DirInfo {
        base_dir: String,
        from_dir: String,
        lang_dir: String,
        proj_dir: String,
        info: Info,
    };

    impl DirInfo {
        fn fulldir(&self) -> PathBuf {
            Path::new(&self.base_dir).join(&self.from_dir).join(&self.lang_dir).join(&self.proj_dir)
        }
        fn fulldir_str(&self) -> String {
            format!("{}/{}/{}/{}", self.base_dir, self.from_dir, self.lang_dir, self.proj_dir)
        }
    }

    // There are several 2D vectors, according to the language and then index. 
    // First, there are the originals and the translations 2D vectors.
    // Then each one is separated into the ones that uses transifex (_tsfx), and those who don't (_local).

    // Then, regarding transifex, a relationship between the originals and translations is needed.
    //   since a thai translation might have come from english, which might have come from japanese, the actual original text,
    //   the relationship is not straightforward. Each text should point at the other two.
    // So two hashmaps are built. On both of them, the key is the transifex 'done' url.
    //   In the first hashmap the value is a copy of the Info structure itself
    //   In the second hashmap the value is a vector of 'done' urls (other keys) - this is cheap to copy.
    //   So for a given 'done' url key, we can access it's Info structure and also the related translation projects Info structures.

    // Then for each project, the script will work on it's 'tmp' folder, so the original contents arent touched.
    // They are actually copied into tmp/original/ folder, to make things simpler.
    // Then inside tmp/ folder, a folder for each target is created, with the tmp/original/ contents.
    // So each target may work on the files isolated from other projects and from other targets.

    // TODO: also build the projects that are _local (not transifex related).
    // TODO: test projects that are translations and are linked to their original language, but aren't finished.
    //   maybe: basically consider unfinished translations as finished and include the progress info accordingly.

    let active_to_langs = consts.all_langs.iter().fold(HashSet::new(), |mut hs, l| {
        if l.to_active {
            hs.insert(&l.to_dir_name);
            hs
        } else {
            hs
        }
    });
    println!("<{:?}>", active_to_langs);


    // let (originals, translations): (Vec<Vec<DirInfo>>, Vec<Vec<DirInfo>>) = 
    let dirs: Vec<DirInfo> = 
        consts.all_langs.iter().filter_map(|lang| {
            // println!("::lang\n{:?}", lang);
            if !lang.from_active { 
                return None; 
            }

            let from_dir_name = lang.from_dir_name.clone();
            if let None = from_dir_name {
                return None;
            }
            let from_dir_name = from_dir_name.unwrap();
            

            info!("Reading language directory: {}", lang.to_dir_name);
            let dir = fs::read_dir(format!("{}/{}", &consts.all_langs_to_dir, &from_dir_name));

            println!("::dir\n{:?}... {:?}", dir, format!("{}/{}", consts.all_langs_to_dir, &from_dir_name));

            if let Err(e) = dir {
                warn!("Failed to open the contents of {}/{} directory. Error: {}", &from_dir_name, lang.to_dir_name, e);
                return None;
            }
            let oks: Vec<DirInfo> = dir.unwrap().into_iter().filter_map(|lang_dir| {

                let lang_dir = lang_dir
                    .map_err(|e| format!("Failed to open language directory {} due to {}", lang.to_dir_name, e));
                if let Err(_) = lang_dir {
                    return None;
                }
                let lang_dir = lang_dir.unwrap().path();
                let lang_dir_name = lang_dir.file_name().unwrap().to_string_lossy().to_string();

                if !active_to_langs.contains(&lang_dir_name) {
                    return None;
                }
                

                let proj_dirs = fs::read_dir(lang_dir);
                let dir_infos = proj_dirs.unwrap().into_iter().filter_map(|proj_dir| {

                    let proj_dir = proj_dir.unwrap().path();
                    let proj_dir_name = proj_dir.file_name().unwrap().to_string_lossy().to_string();
                    // println!("::{} ", proj_dir_name);
                    let yml = File::open(proj_dir.join("info.yml"))
                        .map_err(|e| format!("Failed to open the yml info file in folder {}. Error: {}", proj_dir_name, e));
                    if let Err(e) = yml {
                        // println!(" >> yml err");
                        return None;
                    }
                    let yml = yml.unwrap();
                    let info = serde_yaml::from_reader(yml)
                        .map_err(|e| format!("Failed to parse the yml info file contents in folder {}. Error: {}", proj_dir_name, e));
                    if let Err(e) = info {
                        // println!(" >> info err <{}>", e);
                        return None;
                    }
                    let info: Info = info.unwrap();
                    let info_ver = Version::parse(&info.version)
                        .map_err(|e| format!("Failed to parse the info version ({}). Error: {}", &info.version, e));
                    if let Err(_) = info_ver {
                        // println!(" >> ver err");
                        return None;
                    }
                    let info_ver = info_ver.unwrap();
                    if info_ver > min_ver {
                        // bail!(format!("Error: version of info yaml file is too low ({} < {})", &info_ver, min_ver));
                        // println!(" >> min ver err");
                        return None;
                    }

                    let dir_info = DirInfo{
                        base_dir: format!("{}", &consts.all_langs_to_dir),
                        from_dir: format!("{}", &from_dir_name),
                        lang_dir: format!("{}", &lang_dir_name),
                        proj_dir: format!("{}", &proj_dir_name),
                        info: info,
                    };

                    return Some(dir_info);

                }).collect::<Vec<_>>();

                // println!("{:?}", dir_infos);


                // TODO: also, for later on, also read the original english one (since from-en wont have a "to-en")

                
                // let dir = fs::read_dir(format!("{}/{}", consts.all_langs_to_dir, &from_dir_name));


                Some(dir_infos)
            }).fold(vec![], |mut vs, v| {
                vs.extend(v);
                vs
            });
            // for e in errs {
            //     warn!("project not read: {}", e.err().unwrap());
            // }
            if let None = oks.iter().next() {
                None
            } else {
                Some(oks.into_iter().collect::<Vec<DirInfo>>()
                    // .into_iter()
                    // .partition(|dir_info| !dir_info.info.translation)
                )
            }
    }).fold(vec![], |mut vs, v| {
        vs.extend(v);
        vs
    });

    println!("\n\n\n{:?}\n\n\n", dirs);


    fn copy_files_except_tmp(from: &str, to: &str) -> Result<()> {
        fs::create_dir_all(to)
            .chain_err(|| format!("Failed to create a new {} directory.", to))?;

        let dir = Path::new(from);
        let dirs = read_dir(&dir)
            .chain_err(|| format!("Failed to start copying {} contents into the tmp directory.", from))?;

        for path in dirs {
            let path = path
                .chain_err(|| format!("Failed to open a file."))?;
            if path.path().ends_with("tmp") {
                continue;
            }
            let dst = Path::new(to).join(path.path().file_name().unwrap());
            let meta = path.metadata()
                .chain_err(|| format!("Failed to read {} metadata.", path.path().display()))?;
            if meta.is_dir() {
                fs::create_dir(&dst)
                    .chain_err(|| format!("Failed to create a new {:?} directory.", &dst))?;
            } else {
                let orig = path.path();
                fs::copy(&orig, &dst)
                    .chain_err(|| format!("Failed to copy {:?} into {:?} folder.", &orig, &dst))?;
            }
        }
        Ok(())
    }

    info!("Clearing every project tmp folder");
    for proj in &dirs {
        let path = proj.fulldir().join("tmp");
        // let path = format!("{}/tmp", proj.fulldir());
        if Path::new(&path).exists() {
            fs::remove_dir_all(&path)
                .map_err(|e| format!("Failed to clear the contents of {}/tmp directory. Due to {}.", proj.fulldir_str(), e))?;
        }
    }

    // bail!("MORREU MAS PASSA BEM...");

    // TODO: a structure that groups some information for the same project for different languages

    'outer: for proj in &dirs {
        info!("Working on project: {:?}\n", &proj);
        copy_files_except_tmp(&proj.fulldir_str(), &format!("{}/tmp/original", &proj.fulldir_str()))
            .map_err(|e| format!("Error when copying files into {}/tmp/dir folder. Due to {}.", &proj.fulldir_str(), e))?;

        // lang information
        let all_langs = consts.all_langs.clone();
        let (def_lang, other_langs) : (Vec<Lang>, Vec<Lang>) =
            all_langs.iter().cloned().partition(|lang| lang.to_dir_name == proj.info.translation.language);
        let def_lang: Lang = def_lang.into_iter().next()
            .chain_err(|| "Failed to get the default language information from preset")?;

        // TODO: other translations information

        for target in proj.info.targets.clone() {
            copy_files_except_tmp(&format!("{}/tmp/original", &proj.fulldir_str()), &format!("{}/tmp/{}", &proj.fulldir_str(), target.name))
                .map_err(|e| format!("Error when copying files from {}/tmp/original into {}/tmp/{}. Due to {}.", 
                    &proj.fulldir_str(), &proj.fulldir_str(), target.name, e))?;

            // let authors = proj.info.persons_id.iter().
            let info2 = Info2 {
                authors: vec![],
                translators: vec![],
                collaborators: vec![],
                thanks: vec![],
                reviewers: vec![],
            };
            let def = Defaults {
                info: proj.info.clone(),
                info2: info2.clone(),
                target: target.name.clone(),
                //
                all_langs: all_langs.clone(),
                def_lang: def_lang.clone(),
                other_langs: other_langs.clone(),
                //
                consts: consts.clone(),
            };

            // if def.info.language != "br" {
            //     continue;
            // }

            let mut rendered = TERA.render("main.tex", &def)
                .chain_err(|| "Failed to render the tex templates")?;
            rendered = RE_FORWARD_ARROW.replace_all(&rendered, "{").to_string();
            debug!("{}", rendered);

            let mut mdok = File::create(format!("{}/tmp/{}/main_ok.tex", &proj.fulldir_str(), target.name))
                .chain_err(|| "Falied to create tex file")?;
            mdok.write_fmt(format_args!("{}", rendered))
                .chain_err(|| "Failed to write on tex file")?;

            info!("TeX file written.");

            let cdpath = fs::canonicalize(format!("{proj}/tmp/{tgt}", proj=&proj.fulldir_str(), tgt=&target.name))
                .chain_err(|| "Failed to canonicalize the working project directory.")?
                .into_os_string().into_string()
                .map_err(|e| format!("Invalid working directory string path. Error: {:?}", e))?;
            //let cmd = format!("xelatex main_ok.tex -include-directory=\"{cd}\" -output-directory=\"{cd}\" -halt-on-error --shell-escape", 
            //let cmd = format!("xelatex \"{cd}\\main_ok.tex\" -halt-on-error --shell-escape", 
            //let cmd = format!("\"cd /d \"{cd}\" && xelatex main_ok.tex -halt-on-error --shell-escape\"", 
            //let cmd = format!("cd ../transifex && ls");
            let cmd = &format!("cd {cd} && xelatex main_ok.tex -halt-on-error --shell-escape",
            //let cmd = OsStr::new(&cmd);
                    cd=&cdpath.replace(" ", "^ ")[4..]);
                    //cd=&proj.dir[2..]);
            println!("Command:\n{:?}", &cmd);
            //println!("Command:\n{}", &cmd);

            //xelatex main_ok.tex -include-directory="C:/Users/Thiago/Desktop/ancap.ch/transifex/from_th/the essay name/tmp/book" -output-directory="C:/Users/Thiago/Desktop/ancap.ch/transifex/from_th/the essay name/tmp/book" -halt-on-error --shell-escape

            for _ in 0..consts.passages {
                let output = Command::new("cmd")
                    .args(&["/C", cmd])
                    //.args(&["/C", cmd.to_str().unwrap()])
                    .output()
                    .expect("failed to execute XeLaTeX process.");
                
                if !output.status.success() {
                    error!("XeLaTeX failed.");
                    println!("status: {}", output.status);
                    println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
                    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));
                    ::std::process::exit(1);
                }

            }

        }
    }
    
    info!("finished..");
    Ok(())
}

fn main() {
    env_logger::init().unwrap();
    if let Err(ref e) = run() {
        use std::io::Write;
        let stderr = &mut ::std::io::stderr();
        let errmsg = "Error writing to stderr";

        writeln!(stderr, "error: {}", e).expect(errmsg);

        for e in e.iter().skip(1) {
            writeln!(stderr, "caused by: {}", e).expect(errmsg);
        }

        // The backtrace is not always generated. Try to run this example
        // with `RUST_BACKTRACE=1`.
        if let Some(backtrace) = e.backtrace() {
            writeln!(stderr, "backtrace: {:?}", backtrace).expect(errmsg);
        }

        ::std::process::exit(1);
    }
}

