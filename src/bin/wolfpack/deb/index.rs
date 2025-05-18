use deko::bufread::AnyDecoder;
use fs_err::File;
use indicatif::ProgressBar;
use parking_lot::Mutex;
use std::io::BufReader;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tantivy::directory::MmapDirectory;
use tantivy::schema::*;
use tantivy::tokenizer::NgramTokenizer;
use tantivy::Index;
use tantivy::IndexWriter;
use tantivy::TantivyDocument;
use wolfpack::deb;

use crate::db::ConnectionArc;
use crate::deb::RepoId;
use crate::Error;

pub fn new_package_index_writer(index_dir: &Path) -> Result<Arc<Mutex<IndexWriter>>, Error> {
    let mut builder = Schema::builder();
    builder.add_i64_field("id", NumericOptions::default().set_stored().set_indexed());
    // TODO custom indexers
    builder.add_text_field(
        "name",
        TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("en_stem")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        ),
    );
    builder.add_text_field(
        "description",
        TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("en_stem")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        ),
    );
    builder.add_text_field(
        "homepage",
        TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("en_stem")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        ),
    );
    let schema = builder.build();
    fs_err::create_dir_all(index_dir)?;
    let directory = MmapDirectory::open(index_dir)?;
    let index = Index::open_or_create(directory, schema)?;
    let mut writer: IndexWriter = index.writer(1_000_000_000)?;
    // TODO delete_term doesn't work
    writer.delete_all_documents()?;
    writer.commit()?;
    Ok(Arc::new(Mutex::new(writer)))
}

pub fn new_files_index_writer(index_dir: &Path) -> Result<Arc<Mutex<IndexWriter>>, Error> {
    let mut builder = Schema::builder();
    builder.add_i64_field("id", NumericOptions::default().set_stored().set_indexed());
    builder.add_text_field(
        "path",
        TextOptions::default().set_stored().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("en_stem")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        ),
    );
    builder.add_text_field(
        "command",
        TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("trigram")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        ),
    );
    let schema = builder.build();
    fs_err::create_dir_all(index_dir)?;
    let directory = MmapDirectory::open(index_dir)?;
    let index = Index::open_or_create(directory, schema)?;
    init_files_tokenizers(&index)?;
    let mut writer: IndexWriter = index.writer(1_000_000_000)?;
    // TODO delete_term doesn't work
    writer.delete_all_documents()?;
    writer.commit()?;
    Ok(Arc::new(Mutex::new(writer)))
}

pub fn init_files_tokenizers(index: &Index) -> Result<(), Error> {
    index
        .tokenizers()
        .register("trigram", NgramTokenizer::new(2, 3, false)?);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn index_packages(
    writer: Arc<Mutex<IndexWriter>>,
    packages_file: &Path,
    repo_dir: &Path,
    base_url: String,
    repo_id: RepoId,
    db_conn: ConnectionArc,
    dependency_resolution_tasks: Arc<Mutex<Vec<Task>>>,
    repo_name: String,
    indexing_progress_bar: Arc<Mutex<ProgressBar>>,
    progress_bar: Arc<Mutex<ProgressBar>>,
) -> Result<(), Error> {
    let id_field = writer.lock().index().schema().get_field("id")?;
    let name_field = writer.lock().index().schema().get_field("name")?;
    let description_field = writer.lock().index().schema().get_field("description")?;
    let homepage_field = writer.lock().index().schema().get_field("homepage")?;
    let mut packages_str = String::new();
    let mut file = AnyDecoder::new(BufReader::new(File::open(packages_file)?));
    file.read_to_string(&mut packages_str)?;
    let packages: deb::PerArchPackages = packages_str.parse()?;
    let packages = packages.into_inner();
    indexing_progress_bar
        .lock()
        .inc_length(packages.len() as u64);
    // Insert the packages into the database.
    for package in packages.iter() {
        let url = format!("{}/{}", base_url, package.filename.display());
        let package_file = repo_dir.join(&package.filename);
        let package_id =
            match db_conn
                .lock()
                .insert_deb_package(package, &url, &package_file, repo_id)
            {
                Ok(id) => id,
                Err(e) => {
                    log::error!("Failed to index {:?}: {e}", package.inner.name.as_str());
                    continue;
                }
            };
        let writer = writer.lock();
        writer.delete_term(Term::from_field_i64(id_field, package_id));
        let mut doc = TantivyDocument::new();
        doc.add_field_value(id_field, &package_id);
        doc.add_field_value(name_field, package.inner.name.as_str());
        doc.add_field_value(description_field, package.inner.description.as_str());
        if let Some(homepage) = package.inner.homepage.as_ref() {
            doc.add_field_value(homepage_field, homepage.as_str());
        }
        writer.add_document(doc)?;
        indexing_progress_bar.lock().inc(1);
    }
    // TODO commit every N documents
    writer.lock().commit()?;
    // Resolve dependencies in batches.
    progress_bar.lock().inc_length(packages.len() as u64);
    let batch_size = 1_000;
    let mut packages = packages;
    while !packages.is_empty() {
        let batch = packages.split_off(packages.len() - batch_size.min(packages.len()));
        let repo_name = repo_name.clone();
        let db_conn = db_conn.clone();
        let progress_bar = progress_bar.clone();
        dependency_resolution_tasks.lock().push(Box::new(move || {
            if let Err(e) = resolve_dependencies(
                &batch,
                repo_name.clone(),
                db_conn.clone(),
                progress_bar.clone(),
            ) {
                log::error!("Failed to resolve dependencies: {e}");
            }
        }));
    }
    // Force update.
    progress_bar.lock().tick();
    Ok(())
}

pub fn index_package_files(
    writer: Arc<Mutex<IndexWriter>>,
    contents_files: &[PathBuf],
    db_conn: ConnectionArc,
    progress_bar: Arc<Mutex<ProgressBar>>,
) -> Result<(), Error> {
    for contents_file in contents_files.iter() {
        if let Err(e) = do_index_package_files(
            writer.clone(),
            contents_file,
            db_conn.clone(),
            progress_bar.clone(),
        ) {
            log::error!("Failed to index {contents_file:?}: {e}");
        }
    }
    // Force update.
    progress_bar.lock().tick();
    Ok(())
}

fn do_index_package_files(
    writer: Arc<Mutex<IndexWriter>>,
    contents_file: &Path,
    db_conn: ConnectionArc,
    progress_bar: Arc<Mutex<ProgressBar>>,
) -> Result<(), Error> {
    let id_field = writer.lock().index().schema().get_field("id")?;
    let path_field = writer.lock().index().schema().get_field("path")?;
    let command_field = writer.lock().index().schema().get_field("command")?;
    let decoder = AnyDecoder::new(BufReader::new(File::open(contents_file)?));
    let contents: Vec<_> = deb::PackageContents::read(BufReader::new(decoder))?
        .into_inner()
        .into_iter()
        .collect();
    progress_bar.lock().inc_length(contents.len() as u64);
    for (package_name, files) in contents.into_iter() {
        let Some(package_id) = db_conn.lock().get_package_id_by_name(&package_name)? else {
            continue;
        };
        for file in files.into_iter() {
            let command = if let Some(parent) = file.parent() {
                match parent.file_name() {
                    Some(dirname) => {
                        if dirname == Path::new("bin") || dirname == Path::new("sbin") {
                            Some(file.file_name().expect("File name exists"))
                        } else {
                            None
                        }
                    }
                    None => None,
                }
            } else {
                None
            };
            let mut doc = TantivyDocument::new();
            doc.add_field_value(id_field, &package_id);
            doc.add_field_value(path_field, file.to_string_lossy().as_ref());
            if let Some(command) = command {
                doc.add_field_value(command_field, command.to_string_lossy().as_ref());
            }
            writer.lock().add_document(doc)?;
        }
        progress_bar.lock().inc(1);
    }
    // TODO commit every N documents
    writer.lock().commit()?;
    Ok(())
}

fn resolve_dependencies(
    packages: &[deb::ExtendedPackage],
    repo_name: String,
    db_conn: ConnectionArc,
    progress_bar: Arc<Mutex<ProgressBar>>,
) -> Result<(), Error> {
    // Per-task read-only connection to make queries in parallel.
    let ro_conn = db_conn.lock().clone_read_only()?;
    let ro_conn = ro_conn.lock();
    for package in packages.iter() {
        for dep in package
            .inner
            .depends
            .iter()
            .chain(package.inner.pre_depends.iter())
        {
            let matches = ro_conn.select_deb_dependencies(&repo_name, dep)?;
            if matches.len() != 1 {
                continue;
            }
            db_conn
                .lock()
                .insert_deb_dependency(&repo_name, &package.inner.name, matches[0].id)?;
        }
        progress_bar.lock().inc(1);
    }
    Ok(())
}

type Task = Box<dyn FnMut() + Send>;
