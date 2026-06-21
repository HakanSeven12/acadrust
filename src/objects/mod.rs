//! Non-graphical objects (OBJECTS section)
//!
//! Objects are non-graphical elements in a DXF file, such as dictionaries,
//! layouts, groups, and other organizational structures.

mod block_visibility;
mod dictionary_variable;
mod group;
mod image_definition;
mod mlinestyle;
mod multileader_style;
mod plot_settings;
mod scale;
mod sort_entities_table;
mod table_style;
mod xrecord;
mod stub_objects;

pub use block_visibility::{BlockVisibilityParameter, BlockVisibilityState};
pub use dictionary_variable::DictionaryVariable;
pub use group::Group;
pub use image_definition::{ImageDefinition, ImageDefinitionReactor, ResolutionUnit};
pub use mlinestyle::{MLineStyle, MLineStyleElement, MLineStyleFlags};
pub use multileader_style::{
    BlockContentConnectionType, LeaderContentType, LeaderDrawOrderType,
    LeaderLinePropertyOverrideFlags, MultiLeaderDrawOrderType, MultiLeaderPathType,
    MultiLeaderPropertyOverrideFlags, MultiLeaderStyle, TextAlignmentType, TextAngleType,
    TextAttachmentDirectionType, TextAttachmentType,
};
pub use plot_settings::{
    PaperMargin, PlotFlags, PlotPaperUnits, PlotRotation, PlotSettings, PlotType, PlotWindow,
    ScaledType, ShadePlotMode, ShadePlotResolutionLevel,
};
pub use scale::Scale;
pub use sort_entities_table::{SortEntsEntry, SortEntitiesTable};
pub use table_style::{
    CellAlignment, RowCellStyle, TableBorderPropertyFlags, TableBorderType, TableCellBorder,
    TableCellStylePropertyFlags, TableFlowDirection, TableStyle, TableStyleFlags,
};
pub use xrecord::{DictionaryCloningFlags, XRecord, XRecordEntry, XRecordValue, XRecordValueType};
pub use stub_objects::{
    VisualStyle, Material, GeoData,
    SpatialFilter, RasterVariables, BookColor, PlaceHolder,
    DictionaryWithDefault, WipeoutVariables, StubObject,
};

use crate::types::Handle;

/// Dictionary object - stores key-value pairs of object handles
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Dictionary {
    /// Unique handle
    pub handle: Handle,
    /// Owner handle (soft pointer)
    pub owner: Handle,
    /// Dictionary entries (key -> handle)
    pub entries: Vec<(String, Handle)>,
    /// Duplicate record cloning flag
    pub duplicate_cloning: i16,
    /// Hard owner flag
    pub hard_owner: bool,
    /// Reactor handles ({ACAD_REACTORS})
    pub reactors: Vec<Handle>,
    /// Extended dictionary handle ({ACAD_XDICTIONARY})
    pub xdictionary_handle: Option<Handle>,
}

impl Dictionary {
    /// Create a new dictionary
    pub fn new() -> Self {
        Self {
            handle: Handle::NULL,
            owner: Handle::NULL,
            entries: Vec::new(),
            duplicate_cloning: 1,
            hard_owner: false,
            reactors: Vec::new(),
            xdictionary_handle: None,
        }
    }

    /// Add an entry to the dictionary
    pub fn add_entry(&mut self, key: impl Into<String>, handle: Handle) {
        self.entries.push((key.into(), handle));
    }

    /// Get a handle by key
    pub fn get(&self, key: &str) -> Option<Handle> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, h)| *h)
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the dictionary is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for Dictionary {
    fn default() -> Self {
        Self::new()
    }
}

/// Layout object - represents a layout (model space or paper space)
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Layout {
    /// Unique handle
    pub handle: Handle,
    /// Owner handle (soft pointer)
    pub owner: Handle,
    /// Layout name
    pub name: String,
    /// Layout flags
    pub flags: i16,
    /// Tab order
    pub tab_order: i16,
    /// Minimum limits
    pub min_limits: (f64, f64),
    /// Maximum limits
    pub max_limits: (f64, f64),
    /// Insertion base point
    pub insertion_base: (f64, f64, f64),
    /// Minimum extents
    pub min_extents: (f64, f64, f64),
    /// Maximum extents
    pub max_extents: (f64, f64, f64),
    /// Elevation (code 146)
    pub elevation: f64,
    /// UCS origin (codes 13/23/33)
    pub ucs_origin: (f64, f64, f64),
    /// UCS X axis direction (codes 16/26/36)
    pub ucs_x_axis: (f64, f64, f64),
    /// UCS Y axis direction (codes 17/27/37)
    pub ucs_y_axis: (f64, f64, f64),
    /// UCS orthographic type (code 76)
    pub ucs_ortho_type: i16,
    /// Associated block record handle
    pub block_record: Handle,
    /// Viewport handle
    pub viewport: Handle,
    /// Reactor handles ({ACAD_REACTORS})
    pub reactors: Vec<Handle>,
    /// Extended dictionary handle ({ACAD_XDICTIONARY})
    pub xdictionary_handle: Option<Handle>,
    /// Raw DXF AcDbPlotSettings group-code pairs for round-trip preservation.
    /// Layouts embed PlotSettings in the DXF LAYOUT object; since our Layout
    /// struct does not duplicate all PlotSettings fields we capture the raw
    /// pairs on read and replay them verbatim on write.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub raw_plot_settings_codes: Option<Vec<(i32, String)>>,
    /// Physical paper width in mm (from embedded PlotSettings, code 44).
    /// Zero means unknown / not read from the file.
    pub paper_width: f64,
    /// Physical paper height in mm (from embedded PlotSettings, code 45).
    /// Zero means unknown / not read from the file.
    pub paper_height: f64,
    /// Plot rotation from PlotSettings (code 73): 0=none, 1=90°, 2=180°, 3=270°.
    pub plot_rotation: i16,

    // ── Remaining embedded PlotSettings fields ──────────────────────────────
    // The LAYOUT object embeds a full PlotSettings record. Preserving only the
    // paper size left the sheet unsized in AutoCAD (rendered tiny in the corner
    // because the paper-size name / units / margins were dropped). Keep the rest
    // so the layout round-trips faithfully. See issue #156.
    pub plot_page_name: String,
    pub plot_printer_name: String,
    /// Paper-size name (e.g. "ISO_A4_(210.00_x_297.00_MM)"). AutoCAD renders the
    /// sheet from this; an empty name shows no/wrong paper.
    pub paper_size: String,
    pub plot_view_name: String,
    pub plot_style_sheet: String,
    pub plot_margin_left: f64,
    pub plot_margin_bottom: f64,
    pub plot_margin_right: f64,
    pub plot_margin_top: f64,
    pub plot_origin_x: f64,
    pub plot_origin_y: f64,
    pub plot_window_min_x: f64,
    pub plot_window_min_y: f64,
    pub plot_window_max_x: f64,
    pub plot_window_max_y: f64,
    /// Paper units (code 72): 0=inches, 1=mm, 2=pixels.
    pub plot_paper_units: i16,
    pub plot_type: i16,
    pub plot_scale_numerator: f64,
    pub plot_scale_denominator: f64,
    pub plot_scale_type: i16,
    pub plot_scale_factor: f64,
    pub paper_image_origin_x: f64,
    pub paper_image_origin_y: f64,
    pub shade_plot_mode: i16,
    pub shade_plot_resolution: i16,
    pub shade_plot_dpi: i16,
}

impl Layout {
    /// Create a new layout
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            handle: Handle::NULL,
            owner: Handle::NULL,
            name: name.into(),
            flags: 0,
            tab_order: 0,
            min_limits: (0.0, 0.0),
            max_limits: (12.0, 9.0),
            insertion_base: (0.0, 0.0, 0.0),
            min_extents: (0.0, 0.0, 0.0),
            max_extents: (12.0, 9.0, 0.0),
            elevation: 0.0,
            ucs_origin: (0.0, 0.0, 0.0),
            ucs_x_axis: (1.0, 0.0, 0.0),
            ucs_y_axis: (0.0, 1.0, 0.0),
            ucs_ortho_type: 0,
            block_record: Handle::NULL,
            viewport: Handle::NULL,
            reactors: Vec::new(),
            xdictionary_handle: None,
            raw_plot_settings_codes: None,
            paper_width: 0.0,
            paper_height: 0.0,
            plot_rotation: 0,
            plot_page_name: String::new(),
            plot_printer_name: String::new(),
            paper_size: String::new(),
            plot_view_name: String::new(),
            plot_style_sheet: String::new(),
            plot_margin_left: 0.0,
            plot_margin_bottom: 0.0,
            plot_margin_right: 0.0,
            plot_margin_top: 0.0,
            plot_origin_x: 0.0,
            plot_origin_y: 0.0,
            plot_window_min_x: 0.0,
            plot_window_min_y: 0.0,
            plot_window_max_x: 0.0,
            plot_window_max_y: 0.0,
            plot_paper_units: 0,
            plot_type: 5,
            plot_scale_numerator: 1.0,
            plot_scale_denominator: 1.0,
            plot_scale_type: 0,
            plot_scale_factor: 1.0,
            paper_image_origin_x: 0.0,
            paper_image_origin_y: 0.0,
            shade_plot_mode: 0,
            shade_plot_resolution: 0,
            shade_plot_dpi: 300,
        }
    }
}

/// Object types
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ObjectType {
    /// Dictionary object
    Dictionary(Dictionary),
    /// Layout object
    Layout(Layout),
    /// XRecord object - extended data storage
    XRecord(XRecord),
    /// Group object - named collection of entities
    Group(Group),
    /// MLineStyle object - multiline style definition
    MLineStyle(MLineStyle),
    /// ImageDefinition object - raster image definition
    ImageDefinition(ImageDefinition),
    /// PlotSettings object - plot configuration
    PlotSettings(PlotSettings),
    /// MultiLeaderStyle object - multileader style definition
    MultiLeaderStyle(MultiLeaderStyle),
    /// TableStyle object - table style definition
    TableStyle(TableStyle),
    /// Scale object - named scale definition
    Scale(Scale),
    /// SortEntitiesTable object - entity draw order
    SortEntitiesTable(SortEntitiesTable),
    /// DictionaryVariable object - named variable in dictionary
    DictionaryVariable(DictionaryVariable),
    /// VisualStyle object
    VisualStyle(VisualStyle),
    /// Material object
    Material(Material),
    /// ImageDefinitionReactor object
    ImageDefinitionReactor(ImageDefinitionReactor),
    /// GeoData object
    GeoData(GeoData),
    /// SpatialFilter object
    SpatialFilter(SpatialFilter),
    /// RasterVariables object
    RasterVariables(RasterVariables),
    /// BookColor (DBCOLOR) object
    BookColor(BookColor),
    /// PlaceHolder object
    PlaceHolder(PlaceHolder),
    /// DictionaryWithDefault object
    DictionaryWithDefault(DictionaryWithDefault),
    /// WipeoutVariables object
    WipeoutVariables(WipeoutVariables),
    /// Unknown object type (stored as raw data)
    Unknown {
        /// Object type name
        type_name: String,
        /// Object handle
        handle: Handle,
        /// Owner handle
        owner: Handle,
        /// Raw DXF object-specific group-code pairs.
        ///
        /// Each entry is `(group_code, value_string)`. When present the
        /// DXF writer emits the object type, handle, owner and these
        /// pairs, reproducing the original object content.
        #[cfg_attr(feature = "serde", serde(skip))]
        raw_dxf_codes: Option<Vec<(i32, String)>>,
        /// Raw DWG merged-stream bytes for verbatim round-trip reconstruction.
        /// Populated by the DWG reader for unrecognised non-entity objects.
        #[cfg_attr(feature = "serde", serde(skip))]
        raw_dwg_data: Option<Vec<u8>>,
        /// DWG handle-stream bit count (needed to reconstruct the correct split).
        raw_dwg_handle_bits: i64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dictionary_creation() {
        let mut dict = Dictionary::new();
        assert!(dict.is_empty());

        dict.add_entry("KEY1", Handle::new(100));
        assert_eq!(dict.len(), 1);
        assert_eq!(dict.get("KEY1"), Some(Handle::new(100)));
        assert_eq!(dict.get("KEY2"), None);
    }

    #[test]
    fn test_layout_creation() {
        let layout = Layout::new("Layout1");
        assert_eq!(layout.name, "Layout1");
        assert_eq!(layout.tab_order, 0);
    }
}


