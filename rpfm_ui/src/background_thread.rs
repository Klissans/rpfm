//---------------------------------------------------------------------------//
// Copyright (c) 2017-2022 Ismael Gutiérrez González. All rights reserved.
//
// This file is part of the Rusted PackFile Manager (RPFM) project,
// which can be found here: https://github.com/Frodo45127/rpfm.
//
// This file is licensed under the MIT license, &which can be &found here:
// https://github.com/Frodo45127/rpfm/blob/master/LICENSE.
//---------------------------------------------------------------------------//

/*!
Module with the background loop.

Basically, this does the heavy load of the program.
!*/

use open::that;
use std::io::Cursor;
use rpfm_extensions::diagnostics::Diagnostics;
use rpfm_lib::error::RLibError;
use rpfm_lib::files::RFile;
use rpfm_lib::files::animpack::AnimPack;
use anyhow::{anyhow, Result};
use crossbeam::channel::Sender;
use rpfm_lib::files::{Container, ContainerPath, db::DB, loc::Loc, RFileDecoded, text::*};
use rpfm_lib::integrations::log::*;
use rayon::prelude::*;
use rpfm_lib::games::{LUA_REPO, LUA_BRANCH, LUA_REMOTE, pfh_file_type};
//use uuid::Uuid;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::env::temp_dir;
use std::fs::{DirBuilder, File};
use std::io::{BufWriter, Read, Write};
use std::path::PathBuf;
use std::thread;

use rpfm_extensions::dependencies::Dependencies;
use rpfm_extensions::optimizer::OptimizableContainer;

use rpfm_lib::files::{DecodeableExtraData, EncodeableExtraData, FileType, pack::*};
use rpfm_lib::games::pfh_file_type::PFHFileType;
use rpfm_lib::integrations::{assembly_kit::*, git::*};
use rpfm_lib::schema::Schema;
use rpfm_lib::utils::*;


//use rpfm_lib::assembly_kit::*;
//use rpfm_lib::diagnostics::Diagnostics;
//use rpfm_lib::dependencies::{Dependencies, DependenciesInfo};
//use rpfm_lib::packedfile::*;
//use rpfm_lib::packedfile::animpack::AnimPack;
//use rpfm_lib::packedfile::table::db::DB;
//use rpfm_lib::packedfile::table::loc::{Loc, TSV_NAME_LOC};
//use rpfm_lib::packfile::{PackFile, ContainerInfo, packedfile::{PackedFile, RFileInfo, RawPackedFile}, ContainerPath, PFHFlags, RESERVED_NAME_NOTES};
//use rpfm_lib::schema::{*, patch::SchemaPatches};
//use rpfm_lib::settings::*;

//use rpfm_lib::tips::Tips;

use crate::app_ui::NewPackedFile;
use crate::backend::*;
use crate::CENTRAL_COMMAND;
use crate::communications::{CentralCommand, Command, Response, THREADS_COMMUNICATION_ERROR};
use crate::GAME_SELECTED;
use crate::locale::{tr, tre};
use crate::packedfile_views::DataSource;
use crate::RPFM_PATH;
use crate::SCHEMA;
use crate::settings_ui::backend::*;
use crate::SUPPORTED_GAMES;
//use crate::views::table::TableType;

/// This is the background loop that's going to be executed in a parallel thread to the UI. No UI or "Unsafe" stuff here.
///
/// All communication between this and the UI thread is done use the `CENTRAL_COMMAND` static.
pub fn background_loop() {

    //---------------------------------------------------------------------------------------//
    // Initializing stuff...
    //---------------------------------------------------------------------------------------//

    // We need two PackFiles:
    // - `pack_file_decoded`: This one will hold our opened PackFile.
    // - `pack_files_decoded_extra`: This one will hold the PackFiles opened for the `add_from_packfile` feature, using their paths as keys.
    let mut pack_file_decoded = Pack::default();
    let mut pack_files_decoded_extra = BTreeMap::new();

    // Preload the default game's dependencies.
    let mut dependencies = Dependencies::default();

    // Load all the tips we have.
    //let mut tips = if let Ok(tips) = Tips::load() { tips } else { Tips::default() };

    //---------------------------------------------------------------------------------------//
    // Looping forever and ever...
    //---------------------------------------------------------------------------------------//
    info!("Background Thread looping around…");
    'background_loop: loop {

        // Wait until you get something through the channel. This hangs the thread until we got something,
        // so it doesn't use processing power until we send it a message.
        let (sender, response): (Sender<Response>, Command) = CENTRAL_COMMAND.recv_background();
        match response {

            // Command to close the thread.
            Command::Exit => return,

            // In case we want to reset the PackFile to his original state (dummy)...
            Command::ResetPackFile => pack_file_decoded = Pack::default(),

            // In case we want to remove a Secondary Packfile from memory...
            Command::RemovePackFileExtra(path) => { pack_files_decoded_extra.remove(&path); },

            // In case we want to create a "New PackFile"...
            Command::NewPackFile => {
                let game_selected = GAME_SELECTED.read().unwrap();
                let pack_version = game_selected.pfh_version_by_file_type(PFHFileType::Mod);
                pack_file_decoded = Pack::new_with_name_and_version("unknown.pack", pack_version);

                if let Some(version_number) = game_selected.game_version_number(&setting_path(&game_selected.game_key_name())) {
                    pack_file_decoded.set_game_version(version_number);
                }
            }

            // In case we want to "Open one or more PackFiles"...
            Command::OpenPackFiles(paths) => {
                match Pack::read_and_merge(&paths, setting_bool("use_lazy_loading"), false) {
                    Ok(pack) => {
                        pack_file_decoded = pack;

                        // Force decoding of table/locs, so they're in memory for the diagnostics to work.
                        if let Some(ref schema) = *SCHEMA.read().unwrap() {
                            let mut decode_extra_data = DecodeableExtraData::default();
                            decode_extra_data.set_schema(Some(&schema));
                            let extra_data = Some(decode_extra_data);

                            let mut files = pack_file_decoded.files_by_type_mut(&[FileType::DB, FileType::Loc]);
                            files.par_iter_mut().for_each(|file| {
                                let _ = file.decode(&extra_data, true, false);
                            });
                        }

                        CentralCommand::send_back(&sender, Response::ContainerInfo(ContainerInfo::from(&pack_file_decoded)));
                    }
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                }
            }

            // In case we want to "Open an Extra PackFile" (for "Add from PackFile")...
            Command::OpenPackFileExtra(path) => {
                match pack_files_decoded_extra.get(&path) {
                    Some(pack) => CentralCommand::send_back(&sender, Response::ContainerInfo(ContainerInfo::from(pack))),
                    None => match Pack::read_and_merge(&[path.to_path_buf()], true, false) {
                         Ok(pack) => {
                            CentralCommand::send_back(&sender, Response::ContainerInfo(ContainerInfo::from(&pack)));
                            pack_files_decoded_extra.insert(path.to_path_buf(), pack);
                        }
                        Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                    }
                }
            }

            // In case we want to "Load All CA PackFiles"...
            Command::LoadAllCAPackFiles => {
                let game_selected = GAME_SELECTED.read().unwrap();
                match Pack::read_and_merge_ca_packs(&game_selected, &setting_path(&game_selected.game_key_name())) {
                    Ok(pack) => {
                        pack_file_decoded = pack;
                        CentralCommand::send_back(&sender, Response::ContainerInfo(ContainerInfo::from(&pack_file_decoded)));
                    }
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                }
            }

            // In case we want to "Save a PackFile"...
            Command::SavePackFile => {
                match pack_file_decoded.save(None) {
                    Ok(_) => CentralCommand::send_back(&sender, Response::ContainerInfo(From::from(&pack_file_decoded))),
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(anyhow!("Error while trying to save the currently open PackFile: {}", error))),
                }
            }

            // In case we want to "Save a PackFile As"...
            Command::SavePackFileAs(path) => {
                match pack_file_decoded.save(Some(&path)) {
                    Ok(_) => CentralCommand::send_back(&sender, Response::ContainerInfo(From::from(&pack_file_decoded))),
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(anyhow!("Error while trying to save the currently open PackFile: {}", error))),
                }
            }

            // If you want to perform a clean&save over a PackFile...
            Command::CleanAndSavePackFileAs(path) => {
                pack_file_decoded.clean_undecoded();
                match pack_file_decoded.save(Some(&path)) {
                    Ok(_) => CentralCommand::send_back(&sender, Response::ContainerInfo(From::from(&pack_file_decoded))),
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(anyhow!("Error while trying to save the currently open PackFile: {}", error))),
                }
            }
            /*

            // In case we want to change the current shortcuts...
            // TODO: Migrate the entire shortcut system to the Qt one.
            Command::SetShortcuts(shortcuts) => {
                match shortcuts.save() {
                    Ok(()) => CentralCommand::send_back(&sender, Response::Success),
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                }
            }*/

            // In case we want to get the data of a PackFile needed to form the TreeView...
            Command::GetPackFileDataForTreeView => {

                // Get the name and the PackedFile list, and send it.
                CentralCommand::send_back(&sender, Response::ContainerInfoVecRFileInfo((
                    From::from(&pack_file_decoded),
                    pack_file_decoded.files().par_iter().map(|(_, file)| From::from(file)).collect(),

                )));
            }

            // In case we want to get the data of a Secondary PackFile needed to form the TreeView...
            Command::GetPackFileExtraDataForTreeView(path) => {

                // Get the name and the PackedFile list, and serialize it.
                match pack_files_decoded_extra.get(&path) {
                    Some(pack_file) => CentralCommand::send_back(&sender, Response::ContainerInfoVecRFileInfo((
                        From::from(pack_file),
                        pack_file_decoded.files().par_iter().map(|(_, file)| From::from(file)).collect(),
                    ))),
                    None => CentralCommand::send_back(&sender, Response::Error(anyhow!("Cannot find extra PackFile with path: {}", path.to_string_lossy()))),
                }
            }

            // In case we want to get the info of one PackedFile from the TreeView.
            Command::GetRFileInfo(path) => {
                CentralCommand::send_back(&sender, Response::OptionRFileInfo(
                    pack_file_decoded.files().get(&path).map(From::from)
                ));
            }

            // In case we want to get the info of more than one PackedFiles from the TreeView.
            Command::GetPackedFilesInfo(paths) => {
                let paths = paths.iter().map(|path| ContainerPath::File(path.to_owned())).collect::<Vec<_>>();
                CentralCommand::send_back(&sender, Response::VecRFileInfo(
                    pack_file_decoded.files_by_paths(&paths).into_iter().map(From::from).collect()
                ));
            }

            // In case we want to launch a global search on a `PackFile`...
            Command::GlobalSearch(mut global_search) => {
                let game_selected = GAME_SELECTED.read().unwrap();
                match *SCHEMA.read().unwrap() {
                    Some(ref schema) => {
                        global_search.search(&game_selected, &schema, &mut pack_file_decoded, &mut dependencies, &[]);
                        let packed_files_info = RFileInfo::info_from_global_search(&global_search, &pack_file_decoded);
                        CentralCommand::send_back(&sender, Response::GlobalSearchVecRFileInfo((global_search, packed_files_info)));
                    }
                    None => {}
                }
            }

            // In case we want to change the current `Game Selected`...
            Command::SetGameSelected(game_selected) => {
                *GAME_SELECTED.write().unwrap() = SUPPORTED_GAMES.game(&game_selected).unwrap();
                let game = GAME_SELECTED.read().unwrap();
                let t = std::time::SystemTime::now();
                dbg!(t.elapsed().unwrap());
                // Try to load the Schema for this game but, before it, PURGE THE DAMN SCHEMA-RELATED CACHE AND REBUILD IT AFTERWARDS.
                let mut files = pack_file_decoded.files_by_type_mut(&[FileType::DB, FileType::Loc]);
                let extra_data = Some(EncodeableExtraData::default());
                files.par_iter_mut().for_each(|file| {
                    let _ = file.encode(&extra_data, false, true, false);
                });

                dbg!(t.elapsed().unwrap());
                // Load the new schema...
                let schema_path = schemas_path().unwrap().join(game.schema_file_name());

                // Quick fix so we can load old schemas. To be removed once 4.0 lands.
                let _ = Schema::update(&schema_path, &PathBuf::from("schemas/patches.ron"), &game.game_key_name());
                *SCHEMA.write().unwrap() = Schema::load(&schema_path).ok();
                dbg!(SCHEMA.write().unwrap().is_some());
                dbg!(t.elapsed().unwrap());

                // Then use it to re-decode the new files.
                if let Some(ref schema) = *SCHEMA.read().unwrap() {
                    //schema.save_json(&schema_path);
                    let mut extra_data = DecodeableExtraData::default();
                    extra_data.set_schema(Some(schema));
                    let extra_data = Some(extra_data);
                    files.par_iter_mut().for_each(|file| {
                        let _ = file.decode(&extra_data, true, false);
                    });
                }
                dbg!(t.elapsed().unwrap());

                // Send a response, so we can unlock the UI.
                CentralCommand::send_back(&sender, Response::Success);

                // If there is a PackFile open, change his id to match the one of the new `Game Selected`.
                if !pack_file_decoded.disk_file_path().is_empty() {
                    let pfh_file_type = *pack_file_decoded.header().pfh_file_type();
                    pack_file_decoded.header_mut().set_pfh_version(game.pfh_version_by_file_type(pfh_file_type));

                    if let Some(version_number) = game.game_version_number(&setting_path(&game.game_key_name())) {
                        pack_file_decoded.set_game_version(version_number);
                    }
                }
                dbg!(t.elapsed().unwrap());
            }

            // In case we want to generate the dependencies cache for our Game Selected...
            Command::GenerateDependenciesCache => {
                if let Some(ref schema) = *SCHEMA.read().unwrap() {
                    let game_selected = GAME_SELECTED.read().unwrap();
                    let game_path = setting_path(&game_selected.game_key_name());
                    let asskit_path = assembly_kit_path().ok();

                    if game_path.is_dir() {
                        match Dependencies::generate_dependencies_cache(&game_selected, &game_path, &asskit_path) {
                            Ok(mut cache) => {
                                let dependencies_path = dependencies_cache_path().unwrap().join(game_selected.dependencies_cache_file_name());
                                match cache.save(&dependencies_path) {
                                    Ok(_) => {
                                        let _ = dependencies.rebuild(schema, pack_file_decoded.dependencies(), Some(&dependencies_path), &game_selected, &game_path);
                                        let dependencies_info = DependenciesInfo::from(&dependencies);
                                        CentralCommand::send_back(&sender, Response::DependenciesInfo(dependencies_info));
                                    },
                                    Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                                }
                            }
                            Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                        }
                    } else {
                        CentralCommand::send_back(&sender, Response::Error(anyhow!("Game Path not configured. Go to <i>'PackFile/Preferences'</i> and configure it.")));
                    }
                } else {
                    CentralCommand::send_back(&sender, Response::Error(anyhow!("There is no Schema for the Game Selected.")));
                }
            }
            /*
            // In case we want to update the Schema for our Game Selected...
            Command::UpdateCurrentSchemaFromAssKit => {
                if let Some(ref mut schema) = *SCHEMA.write().unwrap() {
                    let game_selected = GAME_SELECTED.read().unwrap();
                    let game_path = setting_path(&game_selected.game_key_name());
                    let asskit_path = setting_path(&format!("{}_assembly_kit", game_selected.game_key_name()));
                    let schema_path = schemas_path().unwrap().join(game_selected.schema_file_name());
                    let tables_to_skip = dependencies.vanilla_tables().keys().collect::<Vec<_>>();

                    if let Ok(tables_to_check) = dependencies.db_and_loc_data(true, false, true, false) {
                        match update_schema_from_raw_files(schema, &game_selected, &asskit_path, &schema_path, &tables_to_skip, &tables_to_check) {
                            Ok(_) => CentralCommand::send_back(&sender, Response::Success),
                            Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                        }
                    }
                } else {
                    CentralCommand::send_back(&sender, Response::Error(anyhow!("There is no Schema for the Game Selected.")));
                }
            }*/

            // In case we want to optimize our PackFile...
            Command::OptimizePackFile => {
                if let Some(ref schema) = *SCHEMA.read().unwrap() {
                    match pack_file_decoded.optimize(&mut dependencies, &schema, setting_bool("optimize_not_renamed_packedfiles")) {
                        Ok(paths_to_delete) => CentralCommand::send_back(&sender, Response::HashSetString(paths_to_delete)),
                        Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                    }
                } else {
                    CentralCommand::send_back(&sender, Response::Error(anyhow!("There is no Schema for the Game Selected.")));
                }
            }
/*
            // In case we want to Patch the SiegeAI of a PackFile...
            Command::PatchSiegeAI => {
                match pack_file_decoded.patch_siege_ai() {
                    Ok(result) => CentralCommand::send_back(&sender, Response::StringVecVecString(result)),
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(error))
                }
            }
            */
            // In case we want to change the PackFile's Type...
            Command::SetPackFileType(new_type) => pack_file_decoded.set_pfh_file_type(new_type),

            // In case we want to change the "Include Last Modified Date" setting of the PackFile...
            Command::ChangeIndexIncludesTimestamp(state) => {
                let mut bitmask = pack_file_decoded.bitmask();
                bitmask.set(PFHFlags::HAS_INDEX_WITH_TIMESTAMPS, state);
                pack_file_decoded.set_bitmask(bitmask);
            },
/*
            // In case we want to compress/decompress the PackedFiles of the currently open PackFile...
            Command::ChangeDataIsCompressed(state) => pack_file_decoded.toggle_compression(state),
            */
            // In case we want to get the path of the currently open `PackFile`.
            Command::GetPackFilePath => CentralCommand::send_back(&sender, Response::PathBuf(PathBuf::from(pack_file_decoded.disk_file_path()))),

            // In case we want to get the Dependency PackFiles of our PackFile...
            Command::GetDependencyPackFilesList => CentralCommand::send_back(&sender, Response::VecString(pack_file_decoded.dependencies().to_vec())),

            // In case we want to set the Dependency PackFiles of our PackFile...
            Command::SetDependencyPackFilesList(packs) => { pack_file_decoded.set_dependencies(packs); },

            // In case we want to check if there is a Dependency Database loaded...
            Command::IsThereADependencyDatabase(include_asskit) => {
                let are_dependencies_loaded = dependencies.is_vanilla_data_loaded(include_asskit);
                CentralCommand::send_back(&sender, Response::Bool(are_dependencies_loaded))
            },

            // In case we want to create a PackedFile from scratch...
            Command::NewPackedFile(path, new_packed_file) => {
                let decoded = match new_packed_file {
                    NewPackedFile::AnimPack(_) => {
                        let file = AnimPack::default();
                        RFileDecoded::AnimPack(file)
                    },
                    NewPackedFile::DB(_, table, version) => {
                        if let Some(ref schema) = *SCHEMA.read().unwrap() {
                            match schema.definition_by_name_and_version(&table, version) {
                                Some(definition) => {
                                    let patches = schema.patches_for_table(&table);
                                    let file = DB::new(definition, patches, &table, false);
                                    RFileDecoded::DB(file)
                                }
                                None => {
                                    CentralCommand::send_back(&sender, Response::Error(anyhow!("No definitions found for the table `{}`, version `{}` in the currently loaded schema.", table, version)));
                                    continue;
                                }
                            }
                        } else {
                            CentralCommand::send_back(&sender, Response::Error(anyhow!("There is no Schema for the Game Selected.")));
                            continue;
                        }
                    },
                    NewPackedFile::Loc(_) => {
                        let file = Loc::new(false);
                        RFileDecoded::Loc(file)
                    }
                    NewPackedFile::Text(_, text_type) => {
                        let mut file = Text::default();
                        file.set_format(text_type);
                        RFileDecoded::Text(file)
                    },
                };
                let file = RFile::new_from_decoded(&decoded, 0, &path);
                match pack_file_decoded.insert(file) {
                    Ok(_) => CentralCommand::send_back(&sender, Response::Success),
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                }
            }

            // When we want to add one or more PackedFiles to our PackFile.
            Command::AddPackedFiles(source_paths, destination_paths, paths_to_ignore, import_tables_from_tsv) => {
                let mut added_paths = vec![];
                let mut it_broke = None;

                // If we're going to import TSV, make sure to remove any collision between binary and TSV.
                let paths = if import_tables_from_tsv {
                    source_paths.iter().zip(destination_paths.iter())
                        .filter(|(source, _)| {
                            if let Some(extension) = source.extension() {
                                if extension == "tsv" {
                                    true
                                } else {
                                    let mut path = source.to_path_buf();
                                    path.set_extension("tsv");
                                    source_paths.par_iter().all(|source| source != &path)
                                }
                            } else {
                                let mut path = source.to_path_buf();
                                path.set_extension("tsv");
                                source_paths.par_iter().all(|source| source != &path)
                            }
                        })
                        .collect::<Vec<(&PathBuf, &String)>>()
                } else {
                    source_paths.iter().zip(destination_paths.iter()).collect::<Vec<(&PathBuf, &String)>>()
                };

                let schema = SCHEMA.read().unwrap();

                for (source_path, destination_path) in paths {

                    // Skip ignored paths.
                    if let Some(ref paths_to_ignore) = paths_to_ignore {
                        if paths_to_ignore.iter().any(|x| source_path.starts_with(x)) {
                            continue;
                        }
                    }

                    match pack_file_decoded.insert_file(source_path, destination_path, &schema) {
                        Ok(path) => added_paths.push(path),
                        Err(error) => it_broke = Some(error),
                    }
                }
                if let Some(error) = it_broke {
                    CentralCommand::send_back(&sender, Response::VecContainerPath(added_paths.to_vec()));
                    CentralCommand::send_back(&sender, Response::Error(From::from(error)));
                } else {
                    CentralCommand::send_back(&sender, Response::VecContainerPath(added_paths.to_vec()));
                    CentralCommand::send_back(&sender, Response::Success);
                }

                // Force decoding of table/locs, so they're in memory for the diagnostics to work.
                //if let Some(ref schema) = *SCHEMA.read().unwrap() {
                //    let paths = added_paths.iter().filter_map(|x| if let ContainerPath::File(path) = x { Some(&**path) } else { None }).collect::<Vec<&[String]>>();
                //    let mut packed_files = pack_file_decoded.get_ref_mut_packed_files_by_paths(paths);
                //    packed_files.par_iter_mut()
                //        .filter(|x| [PackedFileType::DB, PackedFileType::Loc].contains(&x.get_packed_file_type(false)))
                //        .for_each(|x| {
                //        let _ = x.decode_no_locks(schema);
                //    });
                //}
            }/*

            // In case we want to add one or more entire folders to our PackFile...
            Command::AddPackedFilesFromFolder(paths, paths_to_ignore, import_tables_from_tsv) => {
                match pack_file_decoded.add_from_folders(&paths, &paths_to_ignore, true, import_tables_from_tsv) {
                    Ok(paths) => {
                        CentralCommand::send_back(&sender, Response::VecContainerPath(paths.iter().filter(|x| !x.is_empty()).map(|x| ContainerPath::File(x.to_vec())).collect()));

                        // Force decoding of table/locs, so they're in memory for the diagnostics to work.
                        if let Some(ref schema) = *SCHEMA.read().unwrap() {
                            let paths = paths.iter().map(|x| &**x).collect::<Vec<&[String]>>();
                            let mut packed_files = pack_file_decoded.get_ref_mut_packed_files_by_paths(paths);
                            packed_files.par_iter_mut()
                                .filter(|x| [PackedFileType::DB, PackedFileType::Loc].contains(&x.get_packed_file_type(false)))
                                .for_each(|x| {
                                let _ = x.decode_no_locks(schema);
                            });
                        }
                    }
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                }

            }*/

            // In case we want to move stuff from one PackFile to another...
            Command::AddPackedFilesFromPackFile((pack_file_path, paths)) => {
                match pack_files_decoded_extra.get(&pack_file_path) {

                    // Try to add the PackedFile to the main PackFile.
                    Some(pack) => {
                        let files = pack.files_by_paths(&paths);
                        for file in files {
                            let _ = pack_file_decoded.insert(file.clone());
                        }

                        CentralCommand::send_back(&sender, Response::VecContainerPath(paths.to_vec()));

                        // Force decoding of table/locs, so they're in memory for the diagnostics to work.
                        if let Some(ref schema) = *SCHEMA.read().unwrap() {
                            let mut decode_extra_data = DecodeableExtraData::default();
                            decode_extra_data.set_schema(Some(&schema));
                            let extra_data = Some(decode_extra_data);

                            let mut files = pack_file_decoded.files_by_type_mut(&[FileType::DB, FileType::Loc]);
                            files.par_iter_mut().for_each(|file| {
                                let _ = file.decode(&extra_data, true, false);
                            });
                        }
                    }
                    None => CentralCommand::send_back(&sender, Response::Error(anyhow!("Cannot find extra PackFile with path: {}", pack_file_path.to_string_lossy()))),
                }
            }

            // In case we want to move stuff from our PackFile to an Animpack...
            Command::AddPackedFilesFromPackFileToAnimpack(anim_pack_path, paths) => {
                let files = pack_file_decoded.files_by_paths(&paths).into_iter().cloned().collect::<Vec<RFile>>();
                match pack_file_decoded.files_mut().get_mut(&anim_pack_path) {
                    Some(file) => {

                        // Try to decode it using lazy_load if enabled.
                        let mut extra_data = DecodeableExtraData::default();
                        extra_data.set_lazy_load(setting_bool("use_lazy_loading"));
                        let _ = file.decode(&Some(extra_data), true, false);

                        match file.decoded_mut() {
                            Ok(decoded) => match decoded {
                                RFileDecoded::AnimPack(anim_pack) => {
                                    for file in files {
                                        let _ = anim_pack.insert(file);
                                    }

                                    CentralCommand::send_back(&sender, Response::VecContainerPath(paths.to_vec()));
                                }
                                _ => CentralCommand::send_back(&sender, Response::Error(anyhow!("We expected {} to be of type {} but found {}. This is either a bug or you did weird things with the game selected.", anim_pack_path, FileType::AnimPack, FileType::from(&*decoded)))),
                            }
                            _ => CentralCommand::send_back(&sender, Response::Error(anyhow!("Failed to decode the file at the following path: {}", anim_pack_path))),
                        }
                    }
                    None => CentralCommand::send_back(&sender, Response::Error(anyhow!("File not found in the Pack: {}.", anim_pack_path))),
                }
            }

            // In case we want to move stuff from an Animpack to our PackFile...
            Command::AddPackedFilesFromAnimpack(anim_pack_path, paths) => {
                let files = match pack_file_decoded.files_mut().get_mut(&anim_pack_path) {
                    Some(file) => {

                        // Try to decode it using lazy_load if enabled.
                        let mut extra_data = DecodeableExtraData::default();
                        extra_data.set_lazy_load(setting_bool("use_lazy_loading"));
                        let _ = file.decode(&Some(extra_data), true, false);

                        match file.decoded_mut() {
                            Ok(decoded) => match decoded {
                                RFileDecoded::AnimPack(anim_pack) => anim_pack.files_by_paths(&paths).into_iter().cloned().collect::<Vec<RFile>>(),
                                _ => {
                                    CentralCommand::send_back(&sender, Response::Error(anyhow!("We expected {} to be of type {} but found {}. This is either a bug or you did weird things with the game selected.", anim_pack_path, FileType::AnimPack, FileType::from(&*decoded))));
                                    continue;
                                },
                            }
                            _ => {
                                CentralCommand::send_back(&sender, Response::Error(anyhow!("Failed to decode the file at the following path: {}", anim_pack_path)));
                                continue;
                            },
                        }
                    }
                    None => {
                        CentralCommand::send_back(&sender, Response::Error(anyhow!("The file with the path {} doesn't exists on the open Pack.", anim_pack_path)));
                        continue;
                    }
                };

                let paths = files.iter().map(|file| file.path_in_container()).collect::<Vec<_>>();
                for file in files {
                    let _ = pack_file_decoded.insert(file);
                }

                CentralCommand::send_back(&sender, Response::VecContainerPath(paths));
            }

            // In case we want to delete files from an Animpack...
            Command::DeleteFromAnimpack((anim_pack_path, paths)) => {
                match pack_file_decoded.files_mut().get_mut(&anim_pack_path) {
                    Some(file) => {

                        // Try to decode it using lazy_load if enabled.
                        let mut extra_data = DecodeableExtraData::default();
                        extra_data.set_lazy_load(setting_bool("use_lazy_loading"));
                        let _ = file.decode(&Some(extra_data), true, false);

                        match file.decoded_mut() {
                            Ok(decoded) => match decoded {
                                RFileDecoded::AnimPack(anim_pack) => {
                                    for path in paths {
                                        anim_pack.remove(&path);
                                    }

                                    CentralCommand::send_back(&sender, Response::Success);
                                }
                                _ => CentralCommand::send_back(&sender, Response::Error(anyhow!("We expected {} to be of type {} but found {}. This is either a bug or you did weird things with the game selected.", anim_pack_path, FileType::AnimPack, FileType::from(&*decoded)))),
                            }
                            _ => CentralCommand::send_back(&sender, Response::Error(anyhow!("Failed to decode the file at the following path: {}", anim_pack_path))),
                        }
                    }
                    None => CentralCommand::send_back(&sender, Response::Error(anyhow!("File not found in the Pack: {}.", anim_pack_path))),
                }
            }

            // In case we want to decode a RigidModel PackedFile...
            Command::DecodePackedFile(path, data_source) => {
                dbg!(&path);
                dbg!(&data_source);
                match data_source {
                    DataSource::PackFile => {
                        if &path == RESERVED_NAME_NOTES {
                            let mut note = Text::default();
                            note.set_format(TextFormat::Markdown);
                            note.set_contents(pack_file_decoded.notes().to_owned());
                            CentralCommand::send_back(&sender, Response::Text(note));
                        }

                        else {

                            // Find the PackedFile we want and send back the response.
                            match pack_file_decoded.files_mut().get_mut(&path) {
                                Some(file) => {
                                    let mut extra_data = DecodeableExtraData::default();
                                    extra_data.set_lazy_load(setting_bool("use_lazy_loading"));

                                    let schema = SCHEMA.read().unwrap();
                                    extra_data.set_schema(schema.as_ref());

                                    let result = file.decode(&Some(extra_data), true, true).transpose().unwrap();

                                    match result {
                                        Ok(RFileDecoded::AnimFragment(data)) => CentralCommand::send_back(&sender, Response::AnimFragmentRFileInfo(data.clone(), From::from(&*file))),
                                        Ok(RFileDecoded::AnimPack(data)) => CentralCommand::send_back(&sender, Response::AnimPackRFileInfo(From::from(&data), data.files().values().map(|file| From::from(file)).collect(), From::from(&*file))),
                                        Ok(RFileDecoded::AnimsTable(data)) => CentralCommand::send_back(&sender, Response::AnimsTableRFileInfo(data.clone(), From::from(&*file))),
                                        Ok(RFileDecoded::CaVp8(data)) => CentralCommand::send_back(&sender, Response::CaVp8RFileInfo(data.clone(), From::from(&*file))),
                                        Ok(RFileDecoded::ESF(data)) => CentralCommand::send_back(&sender, Response::ESFRFileInfo(data.clone(), From::from(&*file))),
                                        Ok(RFileDecoded::DB(table)) => CentralCommand::send_back(&sender, Response::DBRFileInfo(table.clone(), From::from(&*file))),
                                        Ok(RFileDecoded::Image(image)) => CentralCommand::send_back(&sender, Response::ImageRFileInfo(image.clone(), From::from(&*file))),
                                        Ok(RFileDecoded::Loc(table)) => CentralCommand::send_back(&sender, Response::LocRFileInfo(table.clone(), From::from(&*file))),
                                        Ok(RFileDecoded::MatchedCombat(data)) => CentralCommand::send_back(&sender, Response::MatchedCombatRFileInfo(data.clone(), From::from(&*file))),
                                        Ok(RFileDecoded::RigidModel(rigid_model)) => CentralCommand::send_back(&sender, Response::RigidModelRFileInfo(rigid_model.clone(), From::from(&*file))),
                                        Ok(RFileDecoded::Text(text)) => CentralCommand::send_back(&sender, Response::TextRFileInfo(text.clone(), From::from(&*file))),
                                        Ok(RFileDecoded::UIC(uic)) => CentralCommand::send_back(&sender, Response::UICRFileInfo(uic.clone(), From::from(&*file))),
                                        Ok(RFileDecoded::UnitVariant(_)) => CentralCommand::send_back(&sender, Response::RFileDecodedRFileInfo(result.unwrap().clone(), From::from(&*file))),
                                        Ok(_) => CentralCommand::send_back(&sender, Response::Unknown),
                                        Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                                    }
                                }
                                None => CentralCommand::send_back(&sender, Response::Error(anyhow!("The file with the path {} hasn't been found on this Pack.", path))),
                            }
                        }
                    }

                    DataSource::ParentFiles => {
                        match dependencies.file_mut(&path, false, true) {
                            Ok(file) => {
                                let mut extra_data = DecodeableExtraData::default();
                                extra_data.set_lazy_load(setting_bool("use_lazy_loading"));

                                let schema = SCHEMA.read().unwrap();
                                extra_data.set_schema(schema.as_ref());

                                let result = file.decode(&Some(extra_data), true, true).transpose().unwrap();

                                match result {
                                    Ok(RFileDecoded::AnimFragment(data)) => CentralCommand::send_back(&sender, Response::AnimFragmentRFileInfo(data.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::AnimPack(data)) => CentralCommand::send_back(&sender, Response::AnimPackRFileInfo(From::from(&data), data.files().values().map(|file| From::from(file)).collect(), From::from(&*file))),
                                    Ok(RFileDecoded::AnimsTable(data)) => CentralCommand::send_back(&sender, Response::AnimsTableRFileInfo(data.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::CaVp8(data)) => CentralCommand::send_back(&sender, Response::CaVp8RFileInfo(data.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::ESF(data)) => CentralCommand::send_back(&sender, Response::ESFRFileInfo(data.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::DB(table)) => CentralCommand::send_back(&sender, Response::DBRFileInfo(table.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::Image(image)) => CentralCommand::send_back(&sender, Response::ImageRFileInfo(image.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::Loc(table)) => CentralCommand::send_back(&sender, Response::LocRFileInfo(table.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::MatchedCombat(data)) => CentralCommand::send_back(&sender, Response::MatchedCombatRFileInfo(data.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::RigidModel(rigid_model)) => CentralCommand::send_back(&sender, Response::RigidModelRFileInfo(rigid_model.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::Text(text)) => CentralCommand::send_back(&sender, Response::TextRFileInfo(text.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::UIC(uic)) => CentralCommand::send_back(&sender, Response::UICRFileInfo(uic.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::UnitVariant(_)) => CentralCommand::send_back(&sender, Response::RFileDecodedRFileInfo(result.unwrap().clone(), From::from(&*file))),
                                    Ok(_) => CentralCommand::send_back(&sender, Response::Unknown),
                                    Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                                }
                            }
                            Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                        }
                    }

                    DataSource::GameFiles => {
                        match dependencies.file_mut(&path, true, false) {
                            Ok(file) => {
                                let mut extra_data = DecodeableExtraData::default();
                                extra_data.set_lazy_load(setting_bool("use_lazy_loading"));

                                let schema = SCHEMA.read().unwrap();
                                extra_data.set_schema(schema.as_ref());

                                let result = file.decode(&Some(extra_data), true, true).transpose().unwrap();

                                match result {
                                    Ok(RFileDecoded::AnimFragment(data)) => CentralCommand::send_back(&sender, Response::AnimFragmentRFileInfo(data.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::AnimPack(data)) => CentralCommand::send_back(&sender, Response::AnimPackRFileInfo(From::from(&data), data.files().values().map(|file| From::from(file)).collect(), From::from(&*file))),
                                    Ok(RFileDecoded::AnimsTable(data)) => CentralCommand::send_back(&sender, Response::AnimsTableRFileInfo(data.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::CaVp8(data)) => CentralCommand::send_back(&sender, Response::CaVp8RFileInfo(data.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::ESF(data)) => CentralCommand::send_back(&sender, Response::ESFRFileInfo(data.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::DB(table)) => CentralCommand::send_back(&sender, Response::DBRFileInfo(table.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::Image(image)) => CentralCommand::send_back(&sender, Response::ImageRFileInfo(image.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::Loc(table)) => CentralCommand::send_back(&sender, Response::LocRFileInfo(table.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::MatchedCombat(data)) => CentralCommand::send_back(&sender, Response::MatchedCombatRFileInfo(data.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::RigidModel(rigid_model)) => CentralCommand::send_back(&sender, Response::RigidModelRFileInfo(rigid_model.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::Text(text)) => CentralCommand::send_back(&sender, Response::TextRFileInfo(text.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::UIC(uic)) => CentralCommand::send_back(&sender, Response::UICRFileInfo(uic.clone(), From::from(&*file))),
                                    Ok(RFileDecoded::UnitVariant(_)) => CentralCommand::send_back(&sender, Response::RFileDecodedRFileInfo(result.unwrap().clone(), From::from(&*file))),
                                    Ok(_) => CentralCommand::send_back(&sender, Response::Unknown),
                                    Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                                }
                            }
                            Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                        }
                    }

                    DataSource::AssKitFiles => {
                        let path_split = path.split('/').collect::<Vec<_>>();
                        if path_split.len() > 2 {
                            match dependencies.asskit_only_db_tables().get(path_split[1]) {
                                Some(db) => CentralCommand::send_back(&sender, Response::DBRFileInfo(db.clone(), RFileInfo::default())),
                                None => CentralCommand::send_back(&sender, Response::Error(anyhow!("Table {} not found on Assembly Kit files.", path))),
                            }
                        } else {
                            CentralCommand::send_back(&sender, Response::Error(anyhow!("Path {} doesn't contain an identificable table name.", path)));
                        }
                    }

                    DataSource::ExternalFile => {}
                }
            }

            // When we want to save a PackedFile from the view....
            Command::SavePackedFileFromView(path, file_decoded) => {
                if &path == RESERVED_NAME_NOTES {
                    if let RFileDecoded::Text(data) = file_decoded {
                        pack_file_decoded.set_notes(data.contents().to_owned());
                    }
                }
                else if let Some(file) = pack_file_decoded.files_mut().get_mut(&path) {
                    if let Err(error) = file.set_decoded(file_decoded) {
                        CentralCommand::send_back(&sender, Response::Error(From::from(error)));
                    }
                }
                CentralCommand::send_back(&sender, Response::Success);
            }

            // In case we want to delete PackedFiles from a PackFile...
            Command::DeletePackedFiles(paths) => CentralCommand::send_back(&sender, Response::VecContainerPath(paths.iter().map(|path| pack_file_decoded.remove(&path)).flatten().collect())),

            // In case we want to extract PackedFiles from a PackFile...
            Command::ExtractPackedFiles(container_paths, path, extract_tables_to_tsv) => {
                let schema = SCHEMA.read().unwrap();
                let schema = if extract_tables_to_tsv { &*schema } else { &None };
                let mut errors = 0;
                let mut success = 0;
                for container_path in container_paths {
                    if pack_file_decoded.extract(container_path, &path, true, &schema).is_err() {
                        errors += 1;
                    }else {
                        success += 1;
                    }
                }

                if errors == 0 {
                    CentralCommand::send_back(&sender, Response::String(tre("files_extracted_success", &[&success.to_string()])));
                } else {
                    CentralCommand::send_back(&sender, Response::Error(anyhow!("There were {} errors while extracting.", errors)));
                }
            }

            // In case we want to rename one or more PackedFiles...
            // TODO: make sure we don't pass folders here.
            Command::RenamePackedFiles(renaming_data) => {
                match pack_file_decoded.rename_paths(&renaming_data) {
                    Ok(data) => CentralCommand::send_back(&sender, Response::VecContainerPathContainerPath(data.iter().map(|(x, y)| (x.clone(), y[0].to_owned())).collect::<Vec<_>>())),
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                }
            }/*

            // In case we want to Mass-Import TSV Files...
            Command::MassImportTSV(paths, name) => {
                match pack_file_decoded.mass_import_tsv(&paths, name, true) {
                    Ok(result) => CentralCommand::send_back(&sender, Response::VecVecStringVecVecString(result)),
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                }
            }

            // In case we want to Mass-Export TSV Files...
            Command::MassExportTSV(path_types, path) => {
                match pack_file_decoded.mass_export_tsv(&path_types, &path) {
                    Ok(result) => CentralCommand::send_back(&sender, Response::String(result)),
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                }
            }

            // In case we want to know if a Folder exists, knowing his path...
            Command::FolderExists(path) => {
                CentralCommand::send_back(&sender, Response::Bool(pack_file_decoded.folder_exists(&path)));
            }

            // In case we want to know if PackedFile exists, knowing his path...
            Command::PackedFileExists(path) => {
                CentralCommand::send_back(&sender, Response::Bool(pack_file_decoded.packedfile_exists(&path)));
            }

            // In case we want to get the list of tables in the dependency database...
            Command::GetTableListFromDependencyPackFile => {
                let tables = if let Ok(tables) = dependencies.get_db_and_loc_tables_from_cache(true, false, true, true) {
                    tables.iter().map(|x| x.get_path()[1].to_owned()).collect::<Vec<String>>()
                } else { vec![] };
                CentralCommand::send_back(&sender, Response::VecString(tables));
            }

            // In case we want to get the version of an specific table from the dependency database...
            Command::GetTableVersionFromDependencyPackFile(table_name) => {
                if dependencies.game_has_vanilla_data_loaded(false) {
                    if let Some(ref schema) = *SCHEMA.read().unwrap() {
                        match schema.get_ref_last_definition_db(&table_name, &dependencies) {
                            Ok(definition) => CentralCommand::send_back(&sender, Response::I32(definition.get_version())),
                            Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                        }
                    } else { CentralCommand::send_back(&sender, Response::Error(ErrorKind::SchemaNotFound.into())); }
                } else { CentralCommand::send_back(&sender, Response::Error(ErrorKind::DependenciesCacheNotGeneratedorOutOfDate.into())); }
            }

            // In case we want to get the definition of an specific table from the dependency database...
            Command::GetTableDefinitionFromDependencyPackFile(table_name) => {
                if dependencies.game_has_vanilla_data_loaded(false) {
                    if let Some(ref schema) = *SCHEMA.read().unwrap() {
                        match schema.get_ref_last_definition_db(&table_name, &dependencies) {
                            Ok(definition) => CentralCommand::send_back(&sender, Response::Definition(definition.clone())),
                            Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                        }
                    } else { CentralCommand::send_back(&sender, Response::Error(ErrorKind::SchemaNotFound.into())); }
                } else { CentralCommand::send_back(&sender, Response::Error(ErrorKind::DependenciesCacheNotGeneratedorOutOfDate.into())); }
            }

            // In case we want to merge DB or Loc Tables from a PackFile...
            Command::MergeTables(paths, name, delete_source_files) => {
                match pack_file_decoded.merge_tables(&paths, &name, delete_source_files) {
                    Ok(data) => CentralCommand::send_back(&sender, Response::VecString(data)),
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                }
            }

            // In case we want to update a table...
            Command::UpdateTable(path_type) => {
                if let Some(ref schema) = *SCHEMA.read().unwrap() {
                    if let ContainerPath::File(path) = path_type {
                        if let Some(packed_file) = pack_file_decoded.get_ref_mut_packed_file_by_path(&path) {
                            match packed_file.decode_return_ref_mut_no_locks(schema) {
                                Ok(packed_file_decoded) => match packed_file_decoded.update_table(&dependencies) {
                                    Ok(data) => {

                                        // Save it to binary, so the decoder will load the proper data if we open it with it.
                                        let _ = packed_file.encode_no_load();
                                        CentralCommand::send_back(&sender, Response::I32I32(data))
                                    },
                                    Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                                }
                                Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                            }
                        } else { CentralCommand::send_back(&sender, Response::Error(ErrorKind::PackedFileNotFound.into())); }
                    } else { CentralCommand::send_back(&sender, Response::Error(ErrorKind::PackedFileNotFound.into())); }
                } else { CentralCommand::send_back(&sender, Response::Error(ErrorKind::SchemaNotFound.into())); }
            }

            // In case we want to replace all matches in a Global Search...
            Command::GlobalSearchReplaceMatches(mut global_search, matches) => {
                let _ = global_search.replace_matches(&mut pack_file_decoded, &matches);
                let packed_files_info = global_search.get_results_packed_file_info(&mut pack_file_decoded);
                CentralCommand::send_back(&sender, Response::GlobalSearchVecRFileInfo((global_search, packed_files_info)));
            }

            // In case we want to replace all matches in a Global Search...
            Command::GlobalSearchReplaceAll(mut global_search) => {
                let _ = global_search.replace_all(&mut pack_file_decoded);
                let packed_files_info = global_search.get_results_packed_file_info(&mut pack_file_decoded);
                CentralCommand::send_back(&sender, Response::GlobalSearchVecRFileInfo((global_search, packed_files_info)));
            }*/

            // In case we want to get the reference data for a definition...
            Command::GetReferenceDataFromDefinition(table_name, definition) => {
                dependencies.generate_local_db_references(&pack_file_decoded, &[table_name.to_owned()]);
                let reference_data = dependencies.db_reference_data(&pack_file_decoded, &table_name, &definition);
                CentralCommand::send_back(&sender, Response::HashMapI32TableReferences(reference_data));
            }

            // In case we want to return an entire PackedFile to the UI.
            Command::FileFromLocalPack(path) => CentralCommand::send_back(&sender, Response::OptionRFile(pack_file_decoded.files().get(&path).cloned())),

            // In case we want to change the format of a ca_vp8 video...
            Command::SetCaVp8Format(path, format) => {
                match pack_file_decoded.files_mut().get_mut(&path) {
                    Some(ref mut rfile) => {
                        match rfile.decoded_mut() {
                            Ok(data) => {
                                if let RFileDecoded::CaVp8(ref mut data) = data {
                                    data.set_format(format);
                                }
                                // TODO: Put an error here.
                            }
                            Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                        }
                    }
                    None => CentralCommand::send_back(&sender, Response::Error(anyhow!("This Pack doesn't exists as a file in the disk."))),
                }
            },

            // In case we want to save an schema to disk...
            Command::SaveSchema(mut schema) => {
                match schema.save(&schemas_path().unwrap().join(GAME_SELECTED.read().unwrap().schema_file_name())) {
                    Ok(_) => {
                        *SCHEMA.write().unwrap() = Some(schema);
                        CentralCommand::send_back(&sender, Response::Success);
                    },
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                }
            }/*

            // In case we want to clean the cache of one or more PackedFiles...
            Command::CleanCache(paths) => {
                let mut packed_files = pack_file_decoded.get_ref_mut_packed_files_by_paths(paths.iter().map(|x| x.as_ref()).collect::<Vec<&[String]>>());
                packed_files.iter_mut().for_each(|x| { let _ = x.encode_and_clean_cache(); });
            }

            // In case we want to export a PackedFile as a TSV file...
            Command::ExportTSV((internal_path, external_path)) => {
                match pack_file_decoded.get_ref_mut_packed_file_by_path(&internal_path) {
                    Some(packed_file) => match packed_file.get_decoded() {
                        RFileDecoded::DB(data) => match data.export_tsv(&external_path, &internal_path[1], &packed_file.get_path()) {
                            Ok(_) => CentralCommand::send_back(&sender, Response::Success),
                            Err(error) =>  CentralCommand::send_back(&sender, Response::Error(error)),
                        },
                        RFileDecoded::Loc(data) => match data.export_tsv(&external_path, TSV_NAME_LOC, &packed_file.get_path()) {
                            Ok(_) => CentralCommand::send_back(&sender, Response::Success),
                            Err(error) =>  CentralCommand::send_back(&sender, Response::Error(error)),
                        },
                        /*
                        RFileDecoded::DependencyPackFileList(data) => match data.export_tsv(&[external_path]) {
                            Ok(_) => CentralCommand::send_back(&sender, Response::Success),
                            Err(error) =>  CentralCommand::send_back(&sender, Response::Error(error)),
                        },*/
                        _ => unimplemented!()
                    }
                    None => CentralCommand::send_back(&sender, Response::Error(ErrorKind::PackedFileNotFound.into())),
                }
            }

            // In case we want to import a TSV as a PackedFile...
            Command::ImportTSV((internal_path, external_path)) => {
                match *SCHEMA.read().unwrap() {
                    Some(ref schema) => {
                        match pack_file_decoded.get_ref_mut_packed_file_by_path(&internal_path) {
                            Some(packed_file) => match packed_file.get_packed_file_type(false) {
                                PackedFileType::DB => match DB::import_tsv(&schema, &external_path) {
                                    Ok((data, _)) => CentralCommand::send_back(&sender, Response::TableType(TableType::DB(data))),
                                    Err(error) =>  CentralCommand::send_back(&sender, Response::Error(error)),
                                },
                                PackedFileType::Loc => match Loc::import_tsv(&schema, &external_path) {
                                    Ok((data, _)) => CentralCommand::send_back(&sender, Response::TableType(TableType::Loc(data))),
                                    Err(error) =>  CentralCommand::send_back(&sender, Response::Error(error)),
                                },
                                _ => unimplemented!()
                            }
                            None => CentralCommand::send_back(&sender, Response::Error(ErrorKind::PackedFileNotFound.into())),
                        }
                    }
                    None => CentralCommand::send_back(&sender, Response::Error(ErrorKind::SchemaNotFound.into())),
                }
            }*/

            // In case we want to open a PackFile's location in the file manager...
            Command::OpenContainingFolder => {

                // If the path exists, try to open it. If not, throw an error.
                let mut path = PathBuf::from(pack_file_decoded.disk_file_path());
                if path.exists() {
                    path.pop();
                    let _ = open::that(&path);
                    CentralCommand::send_back(&sender, Response::Success);
                }
                else {
                    CentralCommand::send_back(&sender, Response::Error(anyhow!("This Pack doesn't exists as a file in the disk.")));
                }
            },

            // When we want to open a PackedFile in a external program...
            Command::OpenPackedFileInExternalProgram(data_source, path) => {
                match data_source {
                    DataSource::PackFile => {
                        let folder = temp_dir().join(format!("rpfm_{}", pack_file_decoded.disk_file_name()));
                        match pack_file_decoded.extract(path.clone(), &folder, true, &SCHEMA.read().unwrap()) {
                            Ok(_) => {

                                let mut extracted_path = folder.to_path_buf();
                                if let ContainerPath::File(path) = path {
                                    extracted_path.push(path);
                                }

                                let _ = that(&extracted_path);
                                CentralCommand::send_back(&sender, Response::PathBuf(extracted_path));
                            }
                            Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                        }
                    }
                    _ => todo!("Make cases for dependencies."),
                }
            }

            // When we want to save a PackedFile from the external view....
            Command::SavePackedFileFromExternalView(path, external_path) => {

                /*
                pack_file_decoded.insert_file(path, &folder, true, &SCHEMA.read().unwrap());


                match pack_file_decoded.files().get(&path) {
                    Some(packed_file) => {
                        match packed_file.file_type() {

                            // Tables we extract them as TSV.
                            FileType::DB | FileType::Loc => {
                                match *SCHEMA.read().unwrap() {
                                    Some(ref schema) => {
                                        match packed_file.decode_return_ref_mut() {
                                            Ok(data) => {
                                                match data {
                                                    RFileDecoded::DB(ref mut data) => {
                                                        match DB::import_tsv(&schema, &external_path) {
                                                            Ok((new_data, _)) => {
                                                                *data = new_data;
                                                                match packed_file.encode_and_clean_cache() {
                                                                    Ok(_) => CentralCommand::send_back(&sender, Response::Success),
                                                                    Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                                                                }
                                                            }
                                                            Err(error) =>  CentralCommand::send_back(&sender, Response::Error(error)),
                                                        }
                                                    }
                                                    RFileDecoded::Loc(ref mut data) => {
                                                        match Loc::import_tsv(&schema, &external_path) {
                                                            Ok((new_data, _)) => {
                                                                *data = new_data;
                                                                match packed_file.encode_and_clean_cache() {
                                                                    Ok(_) => CentralCommand::send_back(&sender, Response::Success),
                                                                    Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                                                                }
                                                            }
                                                            Err(error) =>  CentralCommand::send_back(&sender, Response::Error(error)),
                                                        }
                                                    }
                                                    _ => unimplemented!(),
                                                }
                                            },
                                            Err(error) =>  CentralCommand::send_back(&sender, Response::Error(error)),
                                        }
                                    }
                                    None => CentralCommand::send_back(&sender, Response::Error(ErrorKind::SchemaNotFound.into())),
                                }
                            },

                            _ => {
                                match File::open(external_path) {
                                    Ok(mut file) => {
                                        let mut data = vec![];
                                        match file.read_to_end(&mut data) {
                                            Ok(_) => {
                                                packed_file.set_raw_data(&data);
                                                CentralCommand::send_back(&sender, Response::Success);
                                            }
                                            Err(_) => CentralCommand::send_back(&sender, Response::Error(ErrorKind::IOGeneric.into())),
                                        }
                                    }
                                    Err(_) => CentralCommand::send_back(&sender, Response::Error(ErrorKind::IOGeneric.into())),
                                }
                            }
                        }
                    }
                    None => CentralCommand::send_back(&sender, Response::Error(ErrorKind::PackedFileNotFound.into())),
                }
            }

            // When we want to update our schemas...
            Command::UpdateSchemas => {
                match Schema::update_schema_repo() {

                    // If it worked, we have to update the currently open schema with the one we just downloaded and rebuild cache/dependencies with it.
                    Ok(_) => {

                        // Encode the decoded tables with the old schema, then re-decode them with the new one.
                        pack_file_decoded.get_ref_mut_packed_files_by_type(PackedFileType::DB, false).par_iter_mut().for_each(|x| { let _ = x.encode_and_clean_cache(); });
                        *SCHEMA.write().unwrap() = Schema::load(GAME_SELECTED.read().unwrap().get_schema_name()).ok();
                        if let Some(ref schema) = *SCHEMA.read().unwrap() {
                            pack_file_decoded.get_ref_mut_packed_files_by_type(PackedFileType::DB, false).par_iter_mut().for_each(|x| { let _ = x.decode_no_locks(schema); });
                        }

                        // Try to reload the schema patchs. Ignore them if fails due to missing file.
                        if let Ok(schema_patches) = SchemaPatches::load() {
                            *SCHEMA_PATCHES.write().unwrap() = schema_patches;
                        }

                        // Then rebuild the dependencies stuff.
                        if dependencies.game_has_dependencies_generated() {
                            match dependencies.rebuild(pack_file_decoded.get_packfiles_list(), false) {
                                Ok(_) => CentralCommand::send_back(&sender, Response::Success),
                                Err(error) => CentralCommand::send_back(&sender, Response::Error(ErrorKind::SchemaUpdateRebuildError(error.to_string()).into())),
                            }
                        }

                        // Otherwise, just report the schema update success, and don't leave the ui waiting eternally again...
                        else {
                            CentralCommand::send_back(&sender, Response::Success);
                        }
                    },
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                }
            }

            // When we want to update our messages...
            Command::UpdateMessages => {

                // TODO: Properly reload all loaded tips.
                match Tips::update_from_repo() {
                    Ok(_) => CentralCommand::send_back(&sender, Response::Success),
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                }
            }

            // When we want to update our lua setup...
            Command::UpdateLuaAutogen => {
                match get_lua_autogen_path() {
                    Ok(local_path) => {
                        let git_integration = GitIntegration::new(&local_path, LUA_REPO, LUA_BRANCH, LUA_REMOTE);
                        match git_integration.update_repo() {
                            Ok(_) => CentralCommand::send_back(&sender, Response::Success),
                            Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                        }
                    },
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                }*/
            }

            // When we want to update our program...
            Command::UpdateMainProgram => {
                match crate::updater::update_main_program() {
                    Ok(_) => CentralCommand::send_back(&sender, Response::Success),
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                }
            }

            // When we want to update our program...
            Command::TriggerBackupAutosave => {

                // Note: we no longer notify the UI of success or error to not hang it up.
                if let Ok(Some(file)) = oldest_file_in_folder(&backup_autosave_path().unwrap()) {
                    let _ = pack_file_decoded.clone().save(Some(&file));
                }
            }

            // In case we want to perform a diagnostics check...
            Command::DiagnosticsCheck => {
                let game_selected = GAME_SELECTED.read().unwrap().clone();
                let game_path = setting_path(&game_selected.game_key_name());
                let schema = SCHEMA.read().unwrap().clone();

                if let Some(schema) = schema {

                    // Spawn a separate thread so the UI can keep working.
                    //
                    // NOTE: Find a way to not fucking clone dependencies.
                    thread::spawn(clone!(
                        mut dependencies,
                        mut pack_file_decoded => move || {
                        let mut diagnostics = Diagnostics::default();
                        if pack_file_decoded.pfh_file_type() == PFHFileType::Mod ||
                            pack_file_decoded.pfh_file_type() == PFHFileType::Movie {
                            diagnostics.check(&pack_file_decoded, &mut dependencies, &game_selected, &game_path, &[], &schema);
                        }
                        CentralCommand::send_back(&sender, Response::Diagnostics(diagnostics));
                    }));
                }
            }

            Command::DiagnosticsUpdate(mut diagnostics, path_types) => {
                let game_selected = GAME_SELECTED.read().unwrap().clone();
                let game_path = setting_path(&game_selected.game_key_name());
                let schema = SCHEMA.read().unwrap().clone();

                if let Some(schema) = schema {

                    // Spawn a separate thread so the UI can keep working.
                    //
                    // NOTE: Find a way to not fucking clone dependencies.
                    thread::spawn(clone!(
                        mut dependencies,
                        mut pack_file_decoded => move || {
                        if pack_file_decoded.pfh_file_type() == PFHFileType::Mod ||
                            pack_file_decoded.pfh_file_type() == PFHFileType::Movie {
                            diagnostics.check(&pack_file_decoded, &mut dependencies, &game_selected, &game_path, &path_types, &schema);
                        }
                        CentralCommand::send_back(&sender, Response::Diagnostics(diagnostics));
                    }));
                }
            }

            // In case we want to get the open PackFile's Settings...
            Command::GetPackSettings => CentralCommand::send_back(&sender, Response::PackSettings(pack_file_decoded.settings().clone())),
            Command::SetPackSettings(settings) => { pack_file_decoded.set_settings(settings); }

            Command::GetMissingDefinitions => {

                // Test to see if every DB Table can be decoded. This is slow and only useful when
                // a new patch lands and you want to know what tables you need to decode. So, unless you want
                // to decode new tables, leave the setting as false.
                if setting_bool("check_for_missing_table_definitions") {
                    let mut counter = 0;
                    let mut table_list = String::new();
                    if let Some(ref schema) = *SCHEMA.read().unwrap() {
                        let mut extra_data = DecodeableExtraData::default();
                        extra_data.set_schema(Some(schema));
                        let extra_data = Some(extra_data);

                        for packed_file in pack_file_decoded.files_by_type_mut(&[FileType::DB]) {
                            if packed_file.decode(&extra_data, false, false).is_err() {
                                if packed_file.load().is_ok() {
                                    if let Ok(raw_data) = packed_file.cached() {
                                        let mut reader = Cursor::new(raw_data);
                                        if let Ok((_, _, _, entry_count)) = DB::read_header(&mut reader) {
                                            if entry_count > 0 {
                                                counter += 1;
                                                table_list.push_str(&format!("{}, {:?}\n", counter, packed_file.path_in_container_raw()))
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Try to save the file. And I mean "try". Someone seems to love crashing here...
                    let path = RPFM_PATH.to_path_buf().join(PathBuf::from("missing_table_definitions.txt"));

                    if let Ok(file) = File::create(path) {
                        let mut file = BufWriter::new(file);
                        let _ = file.write_all(table_list.as_bytes());
                    }
                }
            }

            // Ignore errors for now.
            Command::RebuildDependencies(rebuild_only_current_mod_dependencies) => {
                if let Some(ref schema) = *SCHEMA.read().unwrap() {
                    let game_selected = GAME_SELECTED.read().unwrap();
                    let game_path = setting_path(&game_selected.game_key_name());
                    let dependencies_file_path = dependencies_cache_path().unwrap().join(game_selected.dependencies_cache_file_name());
                    let file_path = if !rebuild_only_current_mod_dependencies { Some(&*dependencies_file_path) } else { None };

                    let _ = dependencies.rebuild(schema, pack_file_decoded.dependencies(), file_path, &game_selected, &game_path);
                    let dependencies_info = DependenciesInfo::from(&dependencies);
                    CentralCommand::send_back(&sender, Response::DependenciesInfo(dependencies_info));
                } else {
                    CentralCommand::send_back(&sender, Response::Error(anyhow!("There is no Schema for the Game Selected.")));
                }
            },
            /*
            Command::CascadeEdition(editions) => {
                let edited_paths = DB::cascade_edition(&editions, &mut pack_file_decoded);
                let edited_paths_2 = edited_paths.iter().map(|x| &**x).collect::<Vec<&[String]>>();
                let packed_files_info = pack_file_decoded.get_ref_packed_files_by_paths(edited_paths_2).iter().map(|x| RFileInfo::from(*x)).collect::<Vec<RFileInfo>>();
                CentralCommand::send_back(&sender, Response::VecVecStringVecRFileInfo(edited_paths, packed_files_info));
            }*/

            Command::GoToDefinition(ref_table, ref_column, ref_data) => {
                let table_name = format!("{}_tables", ref_table);
                let table_folder = format!("db/{}", table_name);
                let packed_files = pack_file_decoded.files_by_path(&ContainerPath::Folder(table_folder.to_owned()));
                let mut found = false;
                for packed_file in &packed_files {
                    if let Ok(RFileDecoded::DB(data)) = packed_file.decoded() {
                        if let Some((column_index, row_index)) = data.table().rows_containing_data(&ref_column, &ref_data) {
                            CentralCommand::send_back(&sender, Response::DataSourceStringUsizeUsize(DataSource::PackFile, packed_file.path_in_container_raw().to_owned(), column_index, row_index[0]));
                            found = true;
                            break;
                        }
                    }
                }

                if !found {
                    if let Ok(packed_files) = dependencies.db_data(&table_name, false, true) {
                        for packed_file in &packed_files {
                            if let Ok(RFileDecoded::DB(data)) = packed_file.decoded() {
                                if let Some((column_index, row_index)) = data.table().rows_containing_data(&ref_column, &ref_data) {
                                    CentralCommand::send_back(&sender, Response::DataSourceStringUsizeUsize(DataSource::ParentFiles, packed_file.path_in_container_raw().to_owned(), column_index, row_index[0]));
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                }

                if !found {
                    if let Ok(packed_files) = dependencies.db_data(&table_name, true, false) {
                        for packed_file in &packed_files {
                            if let Ok(RFileDecoded::DB(data)) = packed_file.decoded() {
                                if let Some((column_index, row_index)) = data.table().rows_containing_data(&ref_column, &ref_data) {
                                    CentralCommand::send_back(&sender, Response::DataSourceStringUsizeUsize(DataSource::GameFiles, packed_file.path_in_container_raw().to_owned(), column_index, row_index[0]));
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                }

                if !found {
                    let tables = dependencies.asskit_only_db_tables();
                    for (table_name, table) in tables {
                        if table.table_name() == table_name {
                            if let Some((column_index, row_index)) = table.table().rows_containing_data(&ref_column, &ref_data) {
                                let path = format!("{}/ak_data", &table_folder);
                                CentralCommand::send_back(&sender, Response::DataSourceStringUsizeUsize(DataSource::AssKitFiles, path, column_index, row_index[0]));
                                found = true;
                                break;
                            }
                        }
                    }
                }

                if !found {
                    CentralCommand::send_back(&sender, Response::Error(anyhow!(tr("source_data_for_field_not_found"))));
                }
            },/*

            Command::SearchReferences(reference_map, value) => {
                let paths = reference_map.keys().map(|x| ContainerPath::Folder(format!("db/{}", x))).collect::<Vec<ContainerPath>>();
                let packed_files = pack_file_decoded.get_ref_packed_files_by_path_type_unicased(&paths);

                let mut references: Vec<(DataSource, Vec<String>, String, usize, usize)> = vec![];

                // Pass for local tables.
                for (table_name, columns) in &reference_map {
                    for packed_file in &packed_files {
                        if &packed_file.get_path()[1] == table_name {
                            if let Ok(RFileDecoded::DB(data)) = packed_file.get_decoded_from_memory() {
                                for column_name in columns {
                                    if let Some((column_index, row_indexes)) = data.table().get_location_of_reference_data(column_name, &value) {
                                        for row_index in &row_indexes {
                                            references.push((DataSource::PackFile, packed_file.get_path().to_vec(), column_name.to_owned(), column_index, *row_index));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Pass for parent tables.
                for (table_name, columns) in &reference_map {
                    if let Ok(tables) = dependencies.get_db_tables_with_path_from_cache(table_name, false, true) {
                        references.append(&mut tables.par_iter().map(|(path, table)| {
                            let mut references = vec![];
                            for column_name in columns {
                                if let Some((column_index, row_indexes)) = table.table().get_location_of_reference_data(column_name, &value) {
                                    for row_index in &row_indexes {
                                        references.push((DataSource::ParentFiles, path.split('/').map(|x| x.to_owned()).collect::<Vec<String>>(), column_name.to_owned(), column_index, *row_index));
                                    }
                                }
                            }


                            references
                        }).flatten().collect());
                    }
                }

                // Pass for vanilla tables.
                for (table_name, columns) in &reference_map {
                    if let Ok(tables) = dependencies.get_db_tables_with_path_from_cache(table_name, true, false) {
                        references.append(&mut tables.par_iter().map(|(path, table)| {
                            let mut references = vec![];
                            for column_name in columns {
                                if let Some((column_index, row_indexes)) = table.table().get_location_of_reference_data(column_name, &value) {
                                    for row_index in &row_indexes {
                                        references.push((DataSource::GameFiles, path.split('/').map(|x| x.to_owned()).collect::<Vec<String>>(), column_name.to_owned(), column_index, *row_index));
                                    }
                                }
                            }

                            references
                        }).flatten().collect());
                    }
                }

                CentralCommand::send_back(&sender, Response::VecDataSourceVecStringStringUsizeUsize(references));
            },

            Command::GoToLoc(loc_key) => {
                let packed_files = pack_file_decoded.get_ref_packed_files_by_type(PackedFileType::Loc, false);
                let mut found = false;
                for packed_file in &packed_files {
                    if let Ok(RFileDecoded::Loc(data)) = packed_file.get_decoded_from_memory() {
                        if let Some((column_index, row_index)) = data.get_ref_table().get_source_location_of_reference_data("key", &loc_key) {
                            CentralCommand::send_back(&sender, Response::DataSourceVecStringUsizeUsize(DataSource::PackFile, packed_file.get_path().to_vec(), column_index, row_index));
                            found = true;
                            break;
                        }
                    }
                }

                if !found {
                    if let Ok(packed_files) = dependencies.get_db_and_loc_tables_from_cache(false, true, false, true) {
                        for packed_file in &packed_files {
                            if let Ok(RFileDecoded::Loc(data)) = packed_file.get_decoded_from_memory() {
                                if let Some((column_index, row_index)) = data.get_ref_table().get_source_location_of_reference_data("key", &loc_key) {
                                    CentralCommand::send_back(&sender, Response::DataSourceVecStringUsizeUsize(DataSource::ParentFiles, packed_file.get_path().to_vec(), column_index, row_index));
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                }

                if !found {
                    if let Ok(packed_files) = dependencies.get_db_and_loc_tables_from_cache(false, true, true, false) {
                        for packed_file in &packed_files {
                            if let Ok(RFileDecoded::Loc(data)) = packed_file.get_decoded_from_memory() {
                                if let Some((column_index, row_index)) = data.table().get_source_location_of_reference_data("key", &loc_key) {
                                    CentralCommand::send_back(&sender, Response::DataSourceVecStringUsizeUsize(DataSource::GameFiles, packed_file.get_path().to_vec(), column_index, row_index));
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                }

                if !found {
                    CentralCommand::send_back(&sender, Response::Error(anyhow!(tr("loc_key_not_found"))));
                }
            },*/

            Command::GetSourceDataFromLocKey(loc_key) => CentralCommand::send_back(&sender, Response::OptionStringStringString(dependencies.loc_key_source(&loc_key))),
            Command::GetPackFileName => CentralCommand::send_back(&sender, Response::String(pack_file_decoded.disk_file_name())),
            Command::GetPackedFileRawData(path) => {
                match pack_file_decoded.files_mut().get_mut(&path) {
                    Some(ref mut rfile) => {
                        match rfile.load() {
                            Ok(_) => match rfile.cached() {
                                Ok(data) => CentralCommand::send_back(&sender, Response::VecU8(data.to_vec())),
                                Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                            },
                            Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                        }
                    }
                    None => CentralCommand::send_back(&sender, Response::Error(anyhow!("This PackedFile no longer exists in the PackFile."))),
                }
            },

            Command::ImportDependenciesToOpenPackFile(paths_by_data_source) => {
                let mut added_paths = vec![];

                for (data_source, paths) in &paths_by_data_source {
                    let files = match data_source {
                        DataSource::GameFiles => dependencies.files_by_path(paths, true, false),
                        DataSource::ParentFiles => dependencies.files_by_path(paths, false, true),

                        _ => {
                            CentralCommand::send_back(&sender, Response::Error(anyhow!("You can't import files from this source.")));
                            CentralCommand::send_back(&sender, Response::Success);
                            continue 'background_loop;
                        },
                    };

                    for (_, file) in &files {
                        added_paths.push(pack_file_decoded.insert((*file).clone()).unwrap());

                    }
                }

                CentralCommand::send_back(&sender, Response::VecContainerPath(added_paths));
                CentralCommand::send_back(&sender, Response::Success);
            },
            /*
            Command::GetPackedFilesFromAllSources(paths) => {
                let mut packed_files = HashMap::new();

                // Get PackedFiles requested from the Parent Files.
                let mut packed_files_parent = HashMap::new();
                if let Ok((packed_files_decoded, _)) = dependencies.get_packedfiles_from_parent_files_unicased(&paths) {
                    for packed_file in packed_files_decoded {
                        packed_files_parent.insert(packed_file.get_path(), packed_file);
                    }
                    packed_files.insert(DataSource::ParentFiles, packed_files_parent);
                }

                // Get PackedFiles requested from the Game Files.
                let mut packed_files_game = HashMap::new();
                if let Ok((packed_files_decoded, _)) = dependencies.get_packedfiles_from_game_files_unicased(&paths) {
                    for packed_file in packed_files_decoded {
                        packed_files_game.insert(packed_file.get_path(), packed_file);
                    }
                    packed_files.insert(DataSource::GameFiles, packed_files_game);
                }

                // Get PackedFiles requested from the AssKit Files.
                //let mut packed_files_asskit = HashMap::new();
                //if let Ok((packed_files_decoded, _)) = dependencies.get_packedfile_from_asskit_files(&paths) {
                //    for packed_file in packed_files_decoded {
                //        packed_files_asskit.insert(packed_file.get_path().to_vec(), packed_file);
                //    }
                //    packed_files.insert(DataSource::AssKitFiles, packed_files_asskit);
                //}

                // Get PackedFiles requested from the currently open PackFile, if any.
                let mut packed_files_packfile = HashMap::new();
                for packed_file in pack_file_decoded.get_packed_files_by_path_type_unicased(&paths) {
                    packed_files_packfile.insert(packed_file.get_path().to_vec(), packed_file );
                }
                packed_files.insert(DataSource::PackFile, packed_files_packfile);

                // Return the full list of PackedFiles requested, split by source.
                CentralCommand::send_back(&sender, Response::HashMapDataSourceHashMapContainerPathRFile(packed_files));
            },

            Command::GetPackedFilesNamesStartingWitPathFromAllSources(path) => {
                let mut packed_files = HashMap::new();
                let base_path = if let ContainerPath::Folder(ref path) = path { path.to_vec() } else { unimplemented!() };

                // Get PackedFiles requested from the Parent Files.
                let mut packed_files_parent = HashSet::new();
                if let Ok((packed_files_decoded, _)) = dependencies.get_packedfiles_from_parent_files_unicased(&[path.clone()]) {
                    for packed_file in packed_files_decoded {
                        let packed_file_path = packed_file.get_path()[base_path.len() - 1..].to_vec();
                        packed_files_parent.insert(packed_file_path);
                    }
                    packed_files.insert(DataSource::ParentFiles, packed_files_parent);
                }

                // Get PackedFiles requested from the Game Files.
                let mut packed_files_game = HashSet::new();
                if let Ok((packed_files_decoded, _)) = dependencies.get_packedfiles_from_game_files_unicased(&[path.clone()]) {
                    for packed_file in packed_files_decoded {
                        let packed_file_path = packed_file.get_path()[base_path.len() - 1..].to_vec();
                        packed_files_game.insert(packed_file_path);
                    }
                    packed_files.insert(DataSource::GameFiles, packed_files_game);
                }

                // Get PackedFiles requested from the currently open PackFile, if any.
                let mut packed_files_packfile = HashSet::new();
                for packed_file in pack_file_decoded.get_packed_files_by_path_type_unicased(&[path]) {
                    let packed_file_path = packed_file.get_path()[base_path.len() - 1..].to_vec();
                    packed_files_packfile.insert(packed_file_path);
                }
                packed_files.insert(DataSource::PackFile, packed_files_packfile);

                // Return the full list of PackedFile names requested, split by source.
                CentralCommand::send_back(&sender, Response::HashMapDataSourceHashSetContainerPath(packed_files));
            },*/

            Command::SavePackedFilesToPackFileAndClean(files) => {
                let schema = SCHEMA.read().unwrap();
                match &*schema {
                    Some(ref schema) => {

                        // We receive a list of edited PackedFiles. The UI is the one that takes care of editing them to have the data we want where we want.
                        // Also, the UI is responsible for naming them in case they're new. Here we grab them and directly add them into the PackFile.
                        let mut added_paths = vec![];
                        for file in files {
                            if let Ok(path) = pack_file_decoded.insert(file) {
                                added_paths.push(path);
                            }
                        }

                        // Clean up duplicates from overwrites.
                        added_paths.sort();
                        added_paths.dedup();

                        // Then, optimize the PackFile. This should remove any non-edited rows/files.
                        match pack_file_decoded.optimize(&mut dependencies, schema, false) {
                            Ok(paths_to_delete) => CentralCommand::send_back(&sender, Response::VecContainerPathHashSetString(added_paths, paths_to_delete)),
                            Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                        }
                    },
                    None => CentralCommand::send_back(&sender, Response::Error(anyhow!("There is no Schema for the Game Selected."))),
                }
            },
            /*
            Command::GetTipsForPath(path) => {
                let local_tips = tips.get_local_tips_for_path(&path);
                let remote_tips = tips.get_remote_tips_for_path(&path);
                CentralCommand::send_back(&sender, Response::VecTipVecTip(local_tips, remote_tips));
            }

            Command::AddTipToLocalTips(tip) => {
                tips.add_tip_to_local_tips(tip);
                match tips.save() {
                    Ok(_) => CentralCommand::send_back(&sender, Response::Success),
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                }
            }

            Command::DeleteTipById(id) => {
                tips.delete_tip_by_id(id);
                match tips.save() {
                    Ok(_) => CentralCommand::send_back(&sender, Response::Success),
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                }
            }

            Command::PublishTipById(id) => {
                match tips.publish_tip_by_id(id) {
                    Ok(_) => CentralCommand::send_back(&sender, Response::Success),
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                }
            }

            Command::UploadSchemaPatch(patch) => {
                match patch.upload() {
                    Ok(_) => CentralCommand::send_back(&sender, Response::Success),
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                }
            }

            Command::ImportSchemaPatch(patch) => {
                match SCHEMA_PATCHES.write().unwrap().import(patch) {
                    Ok(_) => CentralCommand::send_back(&sender, Response::Success),
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(error)),
                }
            }*/

            Command::GenerateMissingLocData => {
                match pack_file_decoded.generate_missing_loc_data() {
                    Ok(path) => CentralCommand::send_back(&sender, Response::OptionContainerPath(path)),
                    Err(error) => CentralCommand::send_back(&sender, Response::Error(From::from(error))),
                }
            }

            // Initialize the folder for a MyMod, including the folder structure it needs.
            Command::InitializeMyModFolder(mod_name, mod_game)  => {
                let mut mymod_path = setting_path("mymods_base_path");
                if mymod_path.is_dir() {
                    CentralCommand::send_back(&sender, Response::Error(anyhow!("MyMod path is not configured. Configure it in the settings and try again.")));
                    continue;
                }

                mymod_path.push(&mod_game);

                // Just in case the folder doesn't exist, we try to create it.
                if let Err(error) = DirBuilder::new().recursive(true).create(&mymod_path) {
                    CentralCommand::send_back(&sender, Response::Error(anyhow!("Error while creating the MyMod's Game folder: {}.", error.to_string())));
                    continue;
                }

                // We need to create another folder inside the game's folder with the name of the new "MyMod", to store extracted files.
                mymod_path.push(&mod_name);
                if let Err(error) = DirBuilder::new().recursive(true).create(&mymod_path) {
                    CentralCommand::send_back(&sender, Response::Error(anyhow!("Error while creating the MyMod's Assets folder: {}.", error.to_string())));
                    continue;
                };

                // Create a repo inside the MyMod's folder.
                if !setting_bool("disable_mymod_automatic_git_repo") {
                    let git_integration = GitIntegration::new(&mymod_path, "", "", "");
                    if let Err(error) = git_integration.init() {
                        CentralCommand::send_back(&sender, Response::Error(From::from(error)));
                        continue
                    }
                }

                // If the tw_autogen supports the game, create the vscode and sublime configs for lua mods.
                if !setting_bool("disable_mymod_automatic_configs") {
                    if let Ok(lua_autogen_folder) = lua_autogen_game_path(&GAME_SELECTED.read().unwrap()) {
                        let lua_autogen_folder = lua_autogen_folder.to_string_lossy().to_string().replace("\\", "/");

                        let mut vscode_config_path = mymod_path.to_owned();
                        vscode_config_path.push(".vscode");

                        if let Err(error) = DirBuilder::new().recursive(true).create(&vscode_config_path) {
                            CentralCommand::send_back(&sender, Response::Error(anyhow!("Error while creating the VSCode Config folder: {}.", error.to_string())));
                            continue;
                        };

                        // Prepare both config files.
                        let mut sublime_config_path = mymod_path.to_owned();
                        sublime_config_path.push(format!("{}.sublime-project", mymod_path.file_name().unwrap().to_string_lossy()));

                        let mut vscode_extensions_path_file = vscode_config_path.to_owned();
                        vscode_extensions_path_file.push("extensions.json");

                        let mut vscode_config_path_file = vscode_config_path.to_owned();
                        vscode_config_path_file.push("settings.json");

                        if let Ok(file) = File::create(vscode_extensions_path_file) {
                            let mut file = BufWriter::new(file);
                            let _ = file.write_all("
{
    \"recommendations\": [
        \"sumneko.lua\",
        \"formulahendry.code-runner\"
    ],
}".as_bytes());
                                }

                        if let Ok(file) = File::create(vscode_config_path_file) {
                            let mut file = BufWriter::new(file);
                            let _ = file.write_all(format!("
{{
    \"Lua.workspace.library\": [
        \"{folder}/global/\",
        \"{folder}/campaign/\",
        \"{folder}/frontend/\",
        \"{folder}/battle/\"
    ],
    \"Lua.runtime.version\": \"Lua 5.1\",
    \"Lua.completion.autoRequire\": false,
    \"Lua.workspace.preloadFileSize\": 1500,
    \"Lua.workspace.ignoreSubmodules\": false,
    \"Lua.diagnostics.workspaceDelay\": 500,
    \"Lua.diagnostics.workspaceRate\": 40,
    \"Lua.diagnostics.disable\": [
        \"lowercase-global\",
        \"trailing-space\"
    ],
    \"Lua.hint.setType\": true,
    \"Lua.workspace.ignoreDir\": [
        \".vscode\",
        \".git\"
    ]
}}", folder = lua_autogen_folder).as_bytes());
                                }

                        if let Ok(file) = File::create(sublime_config_path) {
                            let mut file = BufWriter::new(file);
                            let _ = file.write_all(format!("
{{
    \"folders\":
    [
        {{
            \"path\": \".\"
        }}
    ],
    \"settings\": {{
        \"Lua.workspace.library\": [
            \"{folder}/global/\",
            \"{folder}/campaign/\",
            \"{folder}/frontend/\",
            \"{folder}/battle/\"
        ],
        \"Lua.runtime.version\": \"Lua 5.1\",
        \"Lua.completion.autoRequire\": false,
        \"Lua.workspace.preloadFileSize\": 1500,
        \"Lua.workspace.ignoreSubmodules\": false,
        \"Lua.diagnostics.workspaceDelay\": 500,
        \"Lua.diagnostics.workspaceRate\": 40,
        \"Lua.diagnostics.disable\": [
            \"lowercase-global\",
            \"trailing-space\"
        ],
        \"Lua.hint.setType\": true,
        \"Lua.workspace.ignoreDir\": [
            \".vscode\",
            \".git\"
        ],
    }}
}}", folder = lua_autogen_folder).as_bytes());
                                }
                            }
                        }

                // Return the name of the MyMod Pack.
                mymod_path.set_extension("pack");
                CentralCommand::send_back(&sender, Response::PathBuf(mymod_path));
            }

            // These two belong to the network thread, not to this one!!!!
            Command::CheckUpdates | Command::CheckSchemaUpdates | Command::CheckMessageUpdates | Command::CheckLuaAutogenUpdates => panic!("{}{:?}", THREADS_COMMUNICATION_ERROR, response),
            _ => {}
        }
    }
}
