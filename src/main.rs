use std::io::{self, stdout, BufReader, Read, Write};
use std::env;
use std::fs::{copy, create_dir_all, read_to_string, write, File};
use std::path::{Path, PathBuf};
use std::process::Command;
use serde_json::{self, from_str, Map, Value};
use zip::ZipArchive;

enum OS {
    Windows,
    Linux,
    OSX
}

struct AssetIndex {
    id: String,
}

struct VersionJSON {
    asset_index: AssetIndex,
    java_version: u64,
    libraries: Vec<LibraryIndex>,
}

struct DownloadIndex {
    path: String,
}

struct Classifiers {
    linux: Option<DownloadIndex>,
    windows: Option<DownloadIndex>,
    osx: Option<DownloadIndex>,
}

struct LibraryIndex {
    artifact: Option<DownloadIndex>,
    classifiers: Option<Classifiers>,
}

fn object_to_download_index(object: &Map<String, Value>) -> DownloadIndex {
    let path = object["path"].as_str().unwrap().to_string();

    DownloadIndex {
        path,
    }
}

impl VersionJSON {
    fn new(json_string: &str) -> Self {
        // Parse json from json_string
        let json: Value = from_str(json_string).unwrap();
        let json = json.as_object().unwrap();

        let asset_index = {
            let asset_index = json["assetIndex"].as_object().unwrap();
            
            let id = asset_index["id"].as_str().unwrap().to_string();

            AssetIndex {
                id,
            }
        };

        let java_version = {
            let java_version = json["javaVersion"].as_object().unwrap();
            java_version["majorVersion"].as_u64().unwrap()
        };

        let libraries: Vec<LibraryIndex> = {
            let libraries = json["libraries"].as_array().unwrap();

            libraries.iter().map(|library| {
                let library = library.as_object().unwrap();

                let downloads = library["downloads"].as_object().unwrap();

                let artifact = if downloads.contains_key("artifact") && !downloads.contains_key("classifiers") {
                    let artifact = downloads["artifact"].as_object().unwrap();

                    Some(object_to_download_index(artifact))
                } else {
                    None
                };

                let classifiers = if downloads.contains_key("classifiers") {
                    let classifiers = downloads["classifiers"].as_object().unwrap();

                    let linux = if classifiers.contains_key("natives-linux") {
                        Some(object_to_download_index(classifiers["natives-linux"].as_object().unwrap()))
                    } else {
                        None
                    };
                    let windows = if classifiers.contains_key("natives-windows") {
                        Some(object_to_download_index(classifiers["natives-windows"].as_object().unwrap()))
                    } else {
                        None
                    };
                    let osx = if classifiers.contains_key("natives-osx") {
                        Some(object_to_download_index(classifiers["natives-osx"].as_object().unwrap()))
                    } else {
                        None
                    };

                    Some(Classifiers {
                        linux,
                        windows,
                        osx
                    })
                } else {
                    None
                };

                LibraryIndex {
                    artifact,
                    classifiers
                }
            }).collect()
        };

        VersionJSON {
            asset_index,
            java_version,
            libraries,
        }
    }
}

fn main() {
    println!("Starting mc-gradle-builder...");

    let mut version_input = String::new();
    let mut directory_input = String::new();

    println!("Getting operating system...");
    let os = get_os();

    println!("Getting .minecraft directory...");
    let minecraft = get_minecraft_dir(&os);

    let versions = minecraft.join("versions");

    // Get a directory from terminal TODO
    print!("Enter a directory< ");
    stdout().flush().unwrap();
    io::stdin().read_line(&mut directory_input).unwrap();
    directory_input = directory_input.replace("\n", "");

    // Get a Minecraft version from terminal
    print!("Enter a Minecraft version< ");
    stdout().flush().unwrap();
    io::stdin().read_line(&mut version_input).unwrap();
    version_input = version_input.replace("\n", "");

    // Get version folder
    let version = versions.join(&version_input);

    // Load version.json to memory
    println!("Parsing version.json...");
    let mut json_string: String = String::new();
    let mut json_file_name = version_input.clone();
    json_file_name.push_str(".json");
    File::open(version.join(json_file_name)).unwrap().read_to_string(&mut json_string).unwrap();

    // Parse version.json and drop json_string
    let version_json = VersionJSON::new(&json_string);
    drop(json_string);

    // Execute commands
    {
        println!("Creating directory_input directory...");
        Command::new("mkdir").arg(&directory_input).output().unwrap();
        std::env::set_current_dir(&directory_input).unwrap();
        
        println!("Initializing gradle project...");
        Command::new("gradle").arg("init").arg("--type").arg("java-application").output().unwrap();
    }

    println!("Creating gradle.build");

    // Create a new String for gradle script
    let mut gradle_script = String::new();

    // Add plugins for java application
    gradle_script.push_str("apply plugin: 'java'\n");
    gradle_script.push_str("apply plugin: 'application'\n");

    // Add dependencies
    {
        gradle_script.push_str("dependencies {\n");
        gradle_script.push_str("    implementation fileTree('runs/libraries')\n");
        gradle_script.push_str("}\n");
    }

    // Add java version
    gradle_script.push_str(format!("sourceCompatibility = {}\n", version_json.java_version).as_str());
    gradle_script.push_str(format!("targetCompatibility = {}\n", version_json.java_version).as_str());

    // Add a runClient task
    gradle_script.push_str("task runClient(type: JavaExec) {\n");
    gradle_script.push_str("    main = 'Start'\n");
    gradle_script.push_str("    classpath = sourceSets.main.runtimeClasspath\n");
    gradle_script.push_str("    jvmArgs = [\"-Djava.library.path=libraries\"]\n");
    gradle_script.push_str("    workingDir = file('runs')\n");
    gradle_script.push_str("}\n");

    // Rewrite build.gradle
    write("build.gradle", gradle_script).unwrap();

    // Wrhite .gitignore
    write(".gitignore", "/runs").unwrap();

    create_dir_all("runs").unwrap();

    // Initialize assets
    {
        let assets = minecraft.join("assets");

        let indexes = assets.join("indexes");
        let objects = assets.join("objects");

        // Create directories
        println!("Creating directory assets/indexes...");
        create_dir_all("runs/assets/indexes").unwrap();

        println!("Creating directory assets/objects...");
        create_dir_all("runs/assets/objects").unwrap();

        // Copy index.json
        println!("Copying index.json...");
        copy(
            indexes.join(format!("{}.json", version_json.asset_index.id.as_str())).to_str().unwrap(),
            format!("runs/assets/indexes/{}.json", version_json.asset_index.id.as_str())
        ).unwrap();

        // Fetch asset index
        println!("Fetching indexes...");
        let body = read_to_string(assets.join("indexes").join(format!("{}.json", version_json.asset_index.id))).unwrap();
        
        // Parse json from response
        let asset_json: Value = from_str(&body).unwrap();

        // Copy objects
        println!("Copying asset objects...");
        asset_json
            .as_object()
            .unwrap()["objects"]
            .as_object()
            .unwrap()
            .into_iter()
            .for_each(|object| {
                let hash = object.1.as_object().unwrap()["hash"].as_str().unwrap();
                let signature = &hash[..2];

                create_dir_all(format!("runs/assets/objects/{}", signature)).unwrap();
                copy(objects.join(signature).join(hash), format!("runs/assets/objects/{}/{}", signature, hash)).unwrap();
        });
    }

    // Initialize libraries
    {
        println!("Copying libraries");
        let libraries = minecraft.join("libraries");

        // Create directories
        create_dir_all("runs/libraries").unwrap();

        // Copy libraries
        version_json.libraries.iter().for_each(|library| {
            if let Some(artifact) = &library.artifact {
                if Path::new(&libraries.join(artifact.path.as_str())).exists() {
                    // create_dir_all(format!("libraries/{}", parse_directory_from_path(&artifact.path))).unwrap();
                    copy(libraries.join(artifact.path.as_str()), format!("runs/libraries/{}", parse_file_name_from_path(&artifact.path))).unwrap();
                }
            }

            if let Some(classifiers) = &library.classifiers {
                let classifier = match os {
                    OS::Linux => &classifiers.linux,
                    OS::Windows => &classifiers.windows,
                    OS::OSX => &classifiers.osx,
                };

                if let Some(classifier) = classifier {
                    if Path::new(&libraries.join(classifier.path.as_str())).exists() {
                        let file = File::open(libraries.join(classifier.path.as_str())).unwrap();
                        let mut archive = ZipArchive::new(BufReader::new(file)).unwrap();
                        archive.extract("runs/libraries").unwrap();
                    }
                }
            }
        });

        // Copy a client.jar
        {
            let client_name = format!("{}.jar", &version_input);
            copy(version.join(&client_name), format!("runs/libraries/{}", &client_name).as_str()).unwrap();
        }
    }
}

fn get_os() -> OS {
    let os_name = env::consts::OS.to_string().to_lowercase();

    match os_name.as_str() {
        os if os.contains("win") => OS::Windows,
        os if os.contains("mac") => OS::OSX,
        os if os.contains("nix") || os.contains("nux") || os.contains("uni") => OS::Linux,
        _ => panic!("Unsupported operating system"),
    }
}

fn get_minecraft_dir(os: &OS) -> PathBuf {
    let user_home = env::var("HOME").unwrap();

    match os {
        OS::Windows => PathBuf::from(user_home).join(".minecraft"),
        OS::OSX => PathBuf::from(user_home).join("Library/Application Support/minecraft"),
        OS::Linux => PathBuf::from(user_home).join(".minecraft"),
    }
}

fn parse_file_name_from_path(path: &String) -> String {
    Path::new(path).file_name().unwrap().to_str().unwrap().to_string()
}
