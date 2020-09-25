//---------------------------------------------------------------------------//
// Copyright (c) 2017-2020 Ismael Gutiérrez González. All rights reserved.
//
// This file is part of the Rusted PackFile Manager (RPFM) project,
// which can be found here: https://github.com/Frodo45127/rpfm.
//
// This file is licensed under the MIT license, which can be found here:
// https://github.com/Frodo45127/rpfm/blob/master/LICENSE.
//---------------------------------------------------------------------------//

/*!
Module with all the code related to the `DiagnosticsUI`.
!*/

use qt_widgets::q_abstract_item_view::{ScrollHint, SelectionMode};
use qt_widgets::QDockWidget;
use qt_widgets::QGroupBox;
use qt_widgets::q_header_view::ResizeMode;
use qt_widgets::QMainWindow;
use qt_widgets::QPushButton;
use qt_widgets::QTableView;
use qt_widgets::QWidget;

use qt_gui::QBrush;
use qt_gui::QColor;
use qt_gui::QListOfQStandardItem;
use qt_gui::QStandardItem;
use qt_gui::QStandardItemModel;

use qt_core::{CaseSensitivity, ContextMenuPolicy, DockWidgetArea, Orientation, SortOrder};
use qt_core::QBox;
use qt_core::QFlags;
use qt_core::q_item_selection_model::SelectionFlag;
use qt_core::QModelIndex;
use qt_core::QRegExp;
use qt_core::QSortFilterProxyModel;
use qt_core::QString;
use qt_core::QVariant;
use qt_core::QPtr;

use cpp_core::Ptr;

use std::rc::Rc;

use rpfm_error::ErrorKind;

use rpfm_lib::diagnostics::{Diagnostic, DiagnosticResult};
use rpfm_lib::packfile::PathType;
use rpfm_lib::SETTINGS;

use rpfm_getset::{GetRef, GetRefMut, Set};

use crate::AppUI;
use crate::communications::Command;
use crate::CENTRAL_COMMAND;
use crate::locale::{qtr, tr};
use crate::pack_tree::{PackTree, get_color_info, get_color_warning, get_color_error, get_color_info_pressed, get_color_warning_pressed, get_color_error_pressed, TreeViewOperation};
use crate::packedfile_views::{View, ViewType};
use crate::packfile_contents_ui::PackFileContentsUI;
use crate::UI_STATE;
use crate::utils::{create_grid_layout, show_dialog};

pub mod connections;
pub mod slots;

//-------------------------------------------------------------------------------//
//                              Enums & Structs
//-------------------------------------------------------------------------------//

/// This struct contains all the pointers we need to access the widgets in the Diagnostics panel.
#[derive(GetRef, GetRefMut, Set)]
pub struct DiagnosticsUI {

    //-------------------------------------------------------------------------------//
    // `Diagnostics` Dock Widget.
    //-------------------------------------------------------------------------------//
    diagnostics_dock_widget: QBox<QDockWidget>,
    diagnostics_table_view: QBox<QTableView>,
    diagnostics_table_filter: QBox<QSortFilterProxyModel>,
    diagnostics_table_model: QBox<QStandardItemModel>,

    //-------------------------------------------------------------------------------//
    // Filters section.
    //-------------------------------------------------------------------------------//
    diagnostics_button_error: QBox<QPushButton>,
    diagnostics_button_warning: QBox<QPushButton>,
    diagnostics_button_info: QBox<QPushButton>,
    diagnostics_button_only_current_packed_file: QBox<QPushButton>,
}

//-------------------------------------------------------------------------------//
//                             Implementations
//-------------------------------------------------------------------------------//

/// Implementation of `DiagnosticsUI`.
impl DiagnosticsUI {

    /// This function creates an entire `DiagnosticsUI` struct.
    pub unsafe fn new(main_window: Ptr<QMainWindow>) -> Self {

        //-----------------------------------------------//
        // `DiagnosticsUI` DockWidget.
        //-----------------------------------------------//
        let diagnostics_dock_widget = QDockWidget::from_q_widget(main_window);
        let diagnostics_dock_inner_widget = QWidget::new_0a();
        let diagnostics_dock_layout = create_grid_layout(diagnostics_dock_inner_widget.static_upcast());
        diagnostics_dock_widget.set_widget(&diagnostics_dock_inner_widget);
        main_window.add_dock_widget_2a(DockWidgetArea::BottomDockWidgetArea, diagnostics_dock_widget.as_ptr());
        diagnostics_dock_widget.set_window_title(&qtr("gen_loc_diagnostics"));

        // Create and configure the filters section.
        let filter_frame = QGroupBox::new();
        let filter_grid = create_grid_layout(filter_frame.static_upcast());
        filter_grid.set_contents_margins_4a(4, 0, 4, 0);

        let diagnostics_button_error = QPushButton::from_q_string(&qtr("diagnostics_button_error"));
        let diagnostics_button_warning = QPushButton::from_q_string(&qtr("diagnostics_button_warning"));
        let diagnostics_button_info = QPushButton::from_q_string(&qtr("diagnostics_button_info"));
        let diagnostics_button_only_current_packed_file = QPushButton::from_q_string(&qtr("diagnostics_button_only_current_packed_file"));
        diagnostics_button_error.set_checkable(true);
        diagnostics_button_warning.set_checkable(true);
        diagnostics_button_info.set_checkable(true);
        diagnostics_button_only_current_packed_file.set_checkable(true);
        diagnostics_button_error.set_checked(true);

        // Hidden until we get this working.
        diagnostics_button_only_current_packed_file.set_visible(false);

        diagnostics_button_info.set_style_sheet(&QString::from_std_str(&format!("
        QPushButton {{
            background-color: {}
        }}
        QPushButton::checked {{
            background-color: {}
        }}", get_color_info(), get_color_info_pressed())));

        diagnostics_button_warning.set_style_sheet(&QString::from_std_str(&format!("
        QPushButton {{
            background-color: {}
        }}
        QPushButton::checked {{
            background-color: {}
        }}", get_color_warning(), get_color_warning_pressed())));

        diagnostics_button_error.set_style_sheet(&QString::from_std_str(&format!("
        QPushButton {{
            background-color: {}
        }}
        QPushButton::checked {{
            background-color: {}
        }}", get_color_error(), get_color_error_pressed())));

        filter_grid.add_widget_5a(&diagnostics_button_error, 0, 0, 1, 1);
        filter_grid.add_widget_5a(&diagnostics_button_warning, 0, 1, 1, 1);
        filter_grid.add_widget_5a(&diagnostics_button_info, 0, 2, 1, 1);
        filter_grid.add_widget_5a(&diagnostics_button_only_current_packed_file, 0, 3, 1, 1);

        let diagnostics_table_view = QTableView::new_0a();
        let diagnostics_table_filter = QSortFilterProxyModel::new_0a();
        let diagnostics_table_model = QStandardItemModel::new_0a();
        diagnostics_table_filter.set_source_model(&diagnostics_table_model);
        diagnostics_table_view.set_model(&diagnostics_table_filter);
        diagnostics_table_view.set_selection_mode(SelectionMode::ExtendedSelection);
        diagnostics_table_view.set_context_menu_policy(ContextMenuPolicy::CustomContextMenu);

        if SETTINGS.read().unwrap().settings_bool["tight_table_mode"] {
            diagnostics_table_view.vertical_header().set_minimum_section_size(22);
            diagnostics_table_view.vertical_header().set_maximum_section_size(22);
            diagnostics_table_view.vertical_header().set_default_section_size(22);
        }

        diagnostics_dock_layout.add_widget_5a(&filter_frame, 0, 0, 1, 1);
        diagnostics_dock_layout.add_widget_5a(&diagnostics_table_view, 1, 0, 1, 1);

        main_window.set_corner(qt_core::Corner::BottomLeftCorner, qt_core::DockWidgetArea::LeftDockWidgetArea);
        main_window.set_corner(qt_core::Corner::BottomRightCorner, qt_core::DockWidgetArea::RightDockWidgetArea);

        Self {

            //-------------------------------------------------------------------------------//
            // `Diagnostics` Dock Widget.
            //-------------------------------------------------------------------------------//
            diagnostics_dock_widget,
            diagnostics_table_view,
            diagnostics_table_filter,
            diagnostics_table_model,

            //-------------------------------------------------------------------------------//
            // Filters section.
            //-------------------------------------------------------------------------------//
            diagnostics_button_error,
            diagnostics_button_warning,
            diagnostics_button_info,
            diagnostics_button_only_current_packed_file,
        }
    }

    /// This function takes care of checking the entire PackFile for errors.
    pub unsafe fn check(diagnostics_ui: &Rc<Self>) {
        if SETTINGS.read().unwrap().settings_bool["enable_diagnostics_tool"] {
            CENTRAL_COMMAND.send_message_qt(Command::DiagnosticsCheck);
            diagnostics_ui.diagnostics_table_model.clear();
            let diagnostics = CENTRAL_COMMAND.recv_message_diagnostics_to_qt_try();
            Self::load_diagnostics_to_ui(diagnostics_ui, diagnostics.get_ref_diagnostics());
            Self::filter_by_level(diagnostics_ui);
            Self::update_level_counts(diagnostics_ui, diagnostics.get_ref_diagnostics());
            UI_STATE.set_diagnostics(&diagnostics);
        }
    }

    /// This function takes care of updating the results of a diagnostics check for the provided paths.
    pub unsafe fn check_on_path(pack_file_contents_ui: &Rc<PackFileContentsUI>, diagnostics_ui: &Rc<Self>, paths: Vec<PathType>) {
        if SETTINGS.read().unwrap().settings_bool["enable_diagnostics_tool"] {
            let diagnostics = UI_STATE.get_diagnostics();
            CENTRAL_COMMAND.send_message_qt(Command::DiagnosticsUpdate((diagnostics, paths)));
            let (diagnostics, packed_files_info) = CENTRAL_COMMAND.recv_message_diagnostics_update_to_qt_try();

            diagnostics_ui.diagnostics_table_model.clear();
            Self::load_diagnostics_to_ui(diagnostics_ui, diagnostics.get_ref_diagnostics());
            pack_file_contents_ui.packfile_contents_tree_view.update_treeview(true, TreeViewOperation::UpdateTooltip(packed_files_info));

            Self::filter_by_level(diagnostics_ui);
            Self::update_level_counts(diagnostics_ui, diagnostics.get_ref_diagnostics());
            UI_STATE.set_diagnostics(&diagnostics);
        }
    }

    /// This function takes care of loading the results of a diagnostic check into the table.
    unsafe fn load_diagnostics_to_ui(diagnostics_ui: &Rc<Self>, diagnostics: &[Diagnostic]) {
        if !diagnostics.is_empty() {
            for diagnostic in diagnostics {
                for result in diagnostic.get_result() {
                    let qlist_boi = QListOfQStandardItem::new();

                    // Create an empty row.
                    let level = QStandardItem::new();
                    let column = QStandardItem::new();
                    let row = QStandardItem::new();
                    let path = QStandardItem::new();
                    let message = QStandardItem::new();

                    let (result_data, result_type, color) = match result {
                        DiagnosticResult::Info(data) => (data, "Info".to_owned(), get_color_info()),
                        DiagnosticResult::Warning(data) => (data, "Warning".to_owned(), get_color_warning()),
                        DiagnosticResult::Error(data) => (data, "Error".to_owned(), get_color_error()),
                    };

                    level.set_background(&QBrush::from_q_color(&QColor::from_q_string(&QString::from_std_str(color))));
                    level.set_text(&QString::from_std_str(result_type));
                    column.set_data_2a(&QVariant::from_uint(result_data.column_number), 2);
                    row.set_data_2a(&QVariant::from_i64(result_data.row_number + 1), 2);
                    path.set_text(&QString::from_std_str(&diagnostic.get_path().join("/")));
                    message.set_text(&QString::from_std_str(&result_data.message));

                    level.set_editable(false);
                    column.set_editable(false);
                    row.set_editable(false);
                    path.set_editable(false);
                    message.set_editable(false);

                    // Add an empty row to the list.
                    qlist_boi.append_q_standard_item(&mut level.into_ptr().as_mut_raw_ptr());
                    qlist_boi.append_q_standard_item(&mut column.into_ptr().as_mut_raw_ptr());
                    qlist_boi.append_q_standard_item(&mut row.into_ptr().as_mut_raw_ptr());
                    qlist_boi.append_q_standard_item(&mut path.into_ptr().as_mut_raw_ptr());
                    qlist_boi.append_q_standard_item(&mut message.into_ptr().as_mut_raw_ptr());

                    // Append the new row.
                    diagnostics_ui.diagnostics_table_model.append_row_q_list_of_q_standard_item(qlist_boi.as_ref());
                }
            }

            diagnostics_ui.diagnostics_table_model.set_header_data_3a(0, Orientation::Horizontal, &QVariant::from_q_string(&qtr("diagnostics_colum_level")));
            diagnostics_ui.diagnostics_table_model.set_header_data_3a(1, Orientation::Horizontal, &QVariant::from_q_string(&qtr("diagnostics_colum_column")));
            diagnostics_ui.diagnostics_table_model.set_header_data_3a(2, Orientation::Horizontal, &QVariant::from_q_string(&qtr("diagnostics_colum_row")));
            diagnostics_ui.diagnostics_table_model.set_header_data_3a(3, Orientation::Horizontal, &QVariant::from_q_string(&qtr("diagnostics_colum_path")));
            diagnostics_ui.diagnostics_table_model.set_header_data_3a(4, Orientation::Horizontal, &QVariant::from_q_string(&qtr("diagnostics_colum_message")));

            // Hide the column number column for tables.
            diagnostics_ui.diagnostics_table_view.hide_column(1);
            diagnostics_ui.diagnostics_table_view.hide_column(2);
            diagnostics_ui.diagnostics_table_view.sort_by_column_2a(3, SortOrder::AscendingOrder);

            diagnostics_ui.diagnostics_table_view.horizontal_header().set_stretch_last_section(true);
            diagnostics_ui.diagnostics_table_view.horizontal_header().resize_sections(ResizeMode::ResizeToContents);
        }
    }

    /// This function tries to open the PackedFile where the selected match is.
    pub unsafe fn open_match(
        app_ui: &Rc<AppUI>,
        pack_file_contents_ui: &Rc<PackFileContentsUI>,
        model_index_filtered: Ptr<QModelIndex>
    ) {

        let tree_view = &pack_file_contents_ui.packfile_contents_tree_view;
        let filter_model: QPtr<QSortFilterProxyModel> = model_index_filtered.model().static_downcast();
        let model: QPtr<QStandardItemModel> = filter_model.source_model().static_downcast();
        let model_index = filter_model.map_to_source(model_index_filtered.as_ref().unwrap());

        // If it's a match, get the path, the position data of the match, and open the PackedFile, scrolling it down.
        let item_path = model.item_2a(model_index.row(), 3);
        let path = item_path.text().to_std_string();
        let path: Vec<String> = path.split(|x| x == '/' || x == '\\').map(|x| x.to_owned()).collect();

        if let Some(pack_file_contents_model_index) = pack_file_contents_ui.packfile_contents_tree_view.expand_treeview_to_item(&path) {
            let pack_file_contents_model_index = pack_file_contents_model_index.as_ref().unwrap();
            let selection_model = tree_view.selection_model();

            // If it's not in the current TreeView Filter we CAN'T OPEN IT.
            //
            // Note: the selection should already trigger the open PackedFile action.
            if pack_file_contents_model_index.is_valid() {
                tree_view.scroll_to_1a(pack_file_contents_model_index);
                selection_model.select_q_model_index_q_flags_selection_flag(pack_file_contents_model_index, QFlags::from(SelectionFlag::ClearAndSelect));

                if let Some(packed_file_view) = UI_STATE.get_open_packedfiles().iter().find(|x| *x.get_ref_path() == path) {
                    match packed_file_view.get_view() {

                        // In case of tables, we have to get the logical row/column of the match and select it.
                        ViewType::Internal(view) => if let View::Table(view) = view {
                            let table_view = view.get_ref_table();
                            let table_view = table_view.get_mut_ptr_table_view_primary();
                            let table_filter: QPtr<QSortFilterProxyModel> = table_view.model().static_downcast();
                            let table_model: QPtr<QStandardItemModel> = table_filter.source_model().static_downcast();
                            let table_selection_model = table_view.selection_model();

                            let row = model.item_2a(model_index.row(), 2).text().to_std_string().parse::<i32>().unwrap() - 1;
                            let column = model.item_2a(model_index.row(), 1).text().to_std_string().parse::<i32>().unwrap();

                            let table_model_index = table_model.index_2a(row, column);
                            let table_model_index_filtered = table_filter.map_from_source(&table_model_index);
                            if table_model_index_filtered.is_valid() {
                                table_view.scroll_to_2a(table_model_index_filtered.as_ref(), ScrollHint::EnsureVisible);
                                table_selection_model.select_q_model_index_q_flags_selection_flag(table_model_index_filtered.as_ref(), QFlags::from(SelectionFlag::ClearAndSelect));
                            }
                        },

                        _ => {},
                    }
                }
            }
        }
        else { show_dialog(app_ui.main_window, ErrorKind::PackedFileNotInFilter, false); }
    }

    pub unsafe fn filter_by_level(diagnostics_ui: &Rc<Self>) {
        let info_state = diagnostics_ui.diagnostics_button_info.is_checked();
        let warning_state = diagnostics_ui.diagnostics_button_warning.is_checked();
        let error_state = diagnostics_ui.diagnostics_button_error.is_checked();
        let pattern = match (info_state, warning_state, error_state) {
            (true, true, true) => "Info|Warning|Error",
            (true, true, false) => "Info|Warning",
            (true, false, true) => "Info|Error",
            (false, true, true) => "Warning|Error",
            (true, false, false) => "Info",
            (false, false, true) => "Error",
            (false, true, false) => "Warning",
            (false, false, false) => "-1",
        };

        let pattern = QRegExp::new_1a(&QString::from_std_str(pattern));

        diagnostics_ui.diagnostics_table_filter.set_filter_case_sensitivity(CaseSensitivity::CaseSensitive);
        diagnostics_ui.diagnostics_table_filter.set_filter_key_column(0);
        diagnostics_ui.diagnostics_table_filter.set_filter_reg_exp_q_reg_exp(&pattern);
    }

    pub unsafe fn update_level_counts(diagnostics_ui: &Rc<Self>, diagnostics: &[Diagnostic]) {
        let info = diagnostics.iter().map(|x| x.get_result().iter().filter(|y| if let DiagnosticResult::Info(_) = y { true } else { false }).count() ).sum::<usize>();
        let warning = diagnostics.iter().map(|x| x.get_result().iter().filter(|y| if let DiagnosticResult::Warning(_) = y { true } else { false }).count() ).sum::<usize>();
        let error = diagnostics.iter().map(|x| x.get_result().iter().filter(|y| if let DiagnosticResult::Error(_) = y { true } else { false }).count() ).sum::<usize>();

        diagnostics_ui.diagnostics_button_info.set_text(&QString::from_std_str(&format!("{} ({})", tr("diagnostics_button_info"), info)));
        diagnostics_ui.diagnostics_button_warning.set_text(&QString::from_std_str(&format!("{} ({})", tr("diagnostics_button_warning"), warning)));
        diagnostics_ui.diagnostics_button_error.set_text(&QString::from_std_str(&format!("{} ({})", tr("diagnostics_button_error"), error)));
    }
}