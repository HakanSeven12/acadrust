//! Central CAD document structure.
//!
//! [`CadDocument`] is the top-level container that holds everything in a
//! drawing: header variables, tables (layers, line types, text styles, …),
//! entities, non-graphical objects, block definitions, and classes.
//!
//! # Creating a document
//!
//! ```rust
//! use acadrust::CadDocument;
//!
//! // Default version (R2018 / AC1032)
//! let doc = CadDocument::new();
//!
//! // Specific version
//! use acadrust::types::DxfVersion;
//! let doc = CadDocument::with_version(DxfVersion::AC1015); // R2000
//! ```

use crate::classes::DxfClassCollection;
use crate::entities::{EntityCommon, EntityType};
use crate::objects::ObjectType;
use crate::tables::*;
use crate::types::{DxfVersion, Color, Handle, Vector2, Vector3};
use crate::Result;
use std::collections::HashMap;

/// DWG header variables containing drawing settings
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HeaderVariables {
    // ==================== Version-specific Flags ====================
    /// REQUIREDVERSIONS (R2013+) - Bit coded required versions
    pub required_versions: i64,
    
    // ==================== Drawing Mode Flags ====================
    /// DIMASO - Associates dimensions with geometry
    pub associate_dimensions: bool,
    /// DIMSHO - Updates dimensions while dragging
    pub update_dimensions_while_dragging: bool,
    /// ORTHOMODE - Orthogonal mode on/off
    pub ortho_mode: bool,
    /// FILLMODE - Fill mode for solids/hatches
    pub fill_mode: bool,
    /// QTEXTMODE - Quick text mode (boxes instead of text)
    pub quick_text_mode: bool,
    /// MIRRTEXT - Mirror text on/off
    pub mirror_text: bool,
    /// REGENMODE - Auto regeneration mode
    pub regen_mode: bool,
    /// LIMCHECK - Limits checking on/off
    pub limit_check: bool,
    /// PLIMCHECK - Paper space limits checking
    pub paper_space_limit_check: bool,
    /// PLINEGEN - Line type pattern generation for polylines
    pub polyline_linetype_generation: bool,
    /// PSLTSCALE - Paper space line type scaling (0=viewport, 1=normal)
    pub paper_space_linetype_scaling: bool,
    /// TILEMODE - Show model space (tile mode)
    pub show_model_space: bool,
    /// USRTIMER - User timer on/off
    pub user_timer: bool,
    /// WORLDVIEW - World view on/off
    pub world_view: bool,
    /// VISRETAIN - Retain xref visibility settings
    pub retain_xref_visibility: bool,
    /// DISPSILH - Silhouette display for 3D objects
    pub display_silhouette: bool,
    /// SPLFRAME - Display spline control polygon
    pub spline_frame: bool,
    /// DELOBJ - Delete source objects for regions/solids
    pub delete_objects: bool,
    /// DRAGMODE - Drag mode (0=off, 1=on request, 2=auto)
    pub drag_mode: i16,
    /// BLIPMODE - Blip mode on/off
    pub blip_mode: bool,
    /// ATTREQ - Attribute entry dialogs
    pub attribute_request: bool,
    /// ATTDIA - Attribute dialog mode
    pub attribute_dialog: bool,
    
    // ==================== Unit Settings ====================
    /// LUNITS - Linear units format (0=Scientific, 1=Decimal, 2=Engineering, 3=Architectural, 4=Fractional)
    pub linear_unit_format: i16,
    /// LUPREC - Linear unit precision (0-8)
    pub linear_unit_precision: i16,
    /// AUNITS - Angular units format (0=Decimal degrees, 1=DMS, 2=Gradians, 3=Radians, 4=Surveyor)
    pub angular_unit_format: i16,
    /// AUPREC - Angular unit precision (0-8)
    pub angular_unit_precision: i16,
    /// INSUNITS - Insertion units (0=Unitless, 1=Inches, 2=Feet, etc.)
    pub insertion_units: i16,
    /// ATTMODE - Attribute display mode (0=off, 1=normal, 2=all)
    pub attribute_visibility: i16,
    /// PDMODE - Point display mode
    pub point_display_mode: i16,
    /// USERI1-5 - User integer variables
    pub user_int1: i16,
    pub user_int2: i16,
    pub user_int3: i16,
    pub user_int4: i16,
    pub user_int5: i16,
    /// COORDS - Coordinate display mode
    pub coords_mode: i16,
    /// OSMODE - Object snap mode bits
    pub object_snap_mode: i32,
    /// PICKSTYLE - Pick style
    pub pick_style: i16,
    /// SPLINETYPE - Spline type (5=quadratic, 6=cubic)
    pub spline_type: i16,
    /// SPLINESEGS - Spline segments for approximation
    pub spline_segments: i16,
    /// SPLINESEGQS - Spline segments for surface fit
    pub spline_segs_surface: i16,
    /// SURFU - Surface U density
    pub surface_u_density: i16,
    /// SURFV - Surface V density
    pub surface_v_density: i16,
    /// SURFTYPE - Surface type
    pub surface_type: i16,
    /// SURFTAB1 - Surface tabulation 1
    pub surface_tab1: i16,
    /// SURFTAB2 - Surface tabulation 2
    pub surface_tab2: i16,
    /// SHADEDGE - Shade edge mode
    pub shade_edge: i16,
    /// SHADEDIF - Shade diffuse percentage
    pub shade_diffuse: i16,
    /// MAXACTVP - Maximum active viewports
    pub max_active_viewports: i16,
    /// ISOLINES - Isolines on surfaces
    pub isolines: i16,
    /// CMLJUST - Multiline justification
    pub multiline_justification: i16,
    /// TEXTQLTY - Text quality for TrueType
    pub text_quality: i16,
    /// SORTENTS - Entity sort flags
    pub sort_entities: i16,
    /// INDEXCTL - Index control flags
    pub index_control: i16,
    /// HIDETEXT - Hide text during HIDE command
    pub hide_text: i16,
    /// XCLIPFRAME - Xref clipping frame visibility
    pub xclip_frame: i16,
    /// HALOGAP - Halo gap percentage
    pub halo_gap: i16,
    /// OBSCOLOR - Obscured line color
    pub obscured_color: i16,
    /// OBSLTYPE - Obscured line type
    pub obscured_linetype: i16,
    /// INTERSECTIONDISPLAY - Intersection polyline display
    pub intersection_display: i16,
    /// INTERSECTIONCOLOR - Intersection polyline color
    pub intersection_color: i16,
    /// DIMASSOC - Dimension associativity (0=no, 1=non-exploded, 2=associative)
    pub dimension_associativity: i16,
    /// PROJECTNAME - Project name
    pub project_name: String,
    
    // ==================== Scale/Size Defaults ====================
    /// LTSCALE - Global linetype scale
    pub linetype_scale: f64,
    /// TEXTSIZE - Default text height
    pub text_height: f64,
    /// TRACEWID - Default trace width
    pub trace_width: f64,
    /// SKETCHINC - Sketch increment
    pub sketch_increment: f64,
    /// THICKNESS - Default thickness
    pub thickness: f64,
    /// PDSIZE - Point display size
    pub point_display_size: f64,
    /// PLINEWID - Default polyline width
    pub polyline_width: f64,
    /// CELTSCALE - Current entity linetype scale
    pub current_entity_linetype_scale: f64,
    /// VIEWTWIST - View twist angle
    pub view_twist: f64,
    /// FILLETRAD - Fillet radius
    pub fillet_radius: f64,
    /// CHAMFERA - Chamfer distance A
    pub chamfer_distance_a: f64,
    /// CHAMFERB - Chamfer distance B
    pub chamfer_distance_b: f64,
    /// CHAMFERC - Chamfer length
    pub chamfer_length: f64,
    /// CHAMFERD - Chamfer angle
    pub chamfer_angle: f64,
    /// ANGBASE - Base angle
    pub angle_base: f64,
    /// ANGDIR - Angular direction (0=counterclockwise, 1=clockwise)
    pub angle_direction: i16,
    /// ELEVATION - Current elevation
    pub elevation: f64,
    /// PELEVATION - Paper space elevation
    pub paper_elevation: f64,
    /// FACETRES - Facet resolution
    pub facet_resolution: f64,
    /// CMLSCALE - Multiline scale
    pub multiline_scale: f64,
    /// USERR1-5 - User real variables
    pub user_real1: f64,
    pub user_real2: f64,
    pub user_real3: f64,
    pub user_real4: f64,
    pub user_real5: f64,
    /// PSVPSCALE - Viewport default view scale factor (R2000+)
    pub viewport_scale_factor: f64,
    /// CANNOSCALE - Name of the current annotation scale for the active
    /// space, e.g. "1:50" (R2008+). Default "1:1".
    pub current_annotation_scale: String,
    /// CANNOSCALEVALUE - Value of the current annotation scale as a
    /// paper/drawing factor: 1:50 -> 0.02, 2:1 -> 2.0 (R2008+). Default 1.0.
    pub annotation_scale_value: f64,
    /// SHADOWPLANELOCATION - Shadow plane Z location
    pub shadow_plane_location: f64,
    /// LOFTANG1 - Loft angle 1
    pub loft_angle1: f64,
    /// LOFTANG2 - Loft angle 2
    pub loft_angle2: f64,
    /// LOFTMAG1 - Loft magnitude 1
    pub loft_magnitude1: f64,
    /// LOFTMAG2 - Loft magnitude 2
    pub loft_magnitude2: f64,
    /// LOFTPARAM - Loft parameters
    pub loft_param: i16,
    /// LOFTNORMALS - Loft normals mode
    pub loft_normals: i16,
    /// LATITUDE - Geographic latitude
    pub latitude: f64,
    /// LONGITUDE - Geographic longitude
    pub longitude: f64,
    /// NORTHDIRECTION - North direction angle
    pub north_direction: f64,
    /// TIMEZONE - Time zone
    pub timezone: i32,
    /// STEPSPERSEC - Steps per second for walk/fly
    pub steps_per_second: f64,
    /// STEPSIZE - Step size for walk/fly
    pub step_size: f64,
    /// LENSLENGTH - Camera lens length
    pub lens_length: f64,
    /// CAMERAHEIGHT - Camera height
    pub camera_height: f64,
    /// CAMERADISPLAY - Camera display mode
    pub camera_display: bool,
    
    // ==================== Current Entity Settings ====================
    /// CECOLOR - Current entity color
    pub current_entity_color: Color,
    /// CELWEIGHT - Current line weight
    pub current_line_weight: i16,
    /// CEPSNTYPE - Current plot style name type
    pub current_plotstyle_type: i16,
    /// ENDCAPS - Line end cap style
    pub end_caps: i16,
    /// JOINSTYLE - Line join style
    pub join_style: i16,
    /// LWDISPLAY - Lineweight display on/off
    pub lineweight_display: bool,
    /// XEDIT - In-place xref editing
    pub xedit: bool,
    /// EXTNAMES - Extended symbol names (R2000+)
    pub extended_names: bool,
    /// PSTYLEMODE - Plot style mode (0=color, 1=named)
    pub plotstyle_mode: bool,
    /// OLESTARTUP - OLE startup
    pub ole_startup: bool,
    
    // ==================== Dimension Variables ====================
    /// DIMSCALE - Overall dimension scale factor
    pub dim_scale: f64,
    /// DIMASZ - Dimension arrow size
    pub dim_arrow_size: f64,
    /// DIMEXO - Extension line offset
    pub dim_ext_line_offset: f64,
    /// DIMDLI - Dimension line increment
    pub dim_line_increment: f64,
    /// DIMEXE - Extension line extension
    pub dim_ext_line_extension: f64,
    /// DIMRND - Dimension rounding
    pub dim_rounding: f64,
    /// DIMDLE - Dimension line extension
    pub dim_line_extension: f64,
    /// DIMTP - Dimension tolerance plus
    pub dim_tolerance_plus: f64,
    /// DIMTM - Dimension tolerance minus
    pub dim_tolerance_minus: f64,
    /// DIMTXT - Dimension text height
    pub dim_text_height: f64,
    /// DIMCEN - Center mark size
    pub dim_center_mark: f64,
    /// DIMTSZ - Tick size
    pub dim_tick_size: f64,
    /// DIMALTF - Alternate unit scale factor
    pub dim_alt_scale: f64,
    /// DIMLFAC - Linear measurements scale factor
    pub dim_linear_scale: f64,
    /// DIMTVP - Text vertical position
    pub dim_text_vertical_pos: f64,
    /// DIMTFAC - Tolerance text height scale factor
    pub dim_tolerance_scale: f64,
    /// DIMGAP - Dimension line gap
    pub dim_line_gap: f64,
    /// DIMALTRND - Alternate units rounding
    pub dim_alt_rounding: f64,
    /// DIMTOL - Tolerance generation on/off
    pub dim_tolerance: bool,
    /// DIMLIM - Limits generation on/off
    pub dim_limits: bool,
    /// DIMTIH - Text inside horizontal
    pub dim_text_inside_horizontal: bool,
    /// DIMTOH - Text outside horizontal
    pub dim_text_outside_horizontal: bool,
    /// DIMSE1 - Suppress extension line 1
    pub dim_suppress_ext1: bool,
    /// DIMSE2 - Suppress extension line 2
    pub dim_suppress_ext2: bool,
    /// DIMTAD - Text above dimension line
    pub dim_text_above: i16,
    /// DIMZIN - Zero suppression
    pub dim_zero_suppression: i16,
    /// DIMAZIN - Alternate zero suppression
    pub dim_alt_zero_suppression: i16,
    /// DIMALT - Alternate units on/off
    pub dim_alternate_units: bool,
    /// DIMALTD - Alternate decimal places
    pub dim_alt_decimal_places: i16,
    /// DIMTOFL - Force line inside
    pub dim_force_line_inside: bool,
    /// DIMSAH - Separate arrow blocks
    pub dim_separate_arrows: bool,
    /// DIMTIX - Force text inside
    pub dim_force_text_inside: bool,
    /// DIMSOXD - Suppress outside extension dim
    pub dim_suppress_outside_ext: bool,
    /// DIMCLRD - Dimension line color
    pub dim_line_color: Color,
    /// DIMCLRE - Extension line color
    pub dim_ext_line_color: Color,
    /// DIMCLRT - Dimension text color
    pub dim_text_color: Color,
    /// DIMADEC - Angular decimal places
    pub dim_angular_decimal_places: i16,
    /// DIMDEC - Decimal places
    pub dim_decimal_places: i16,
    /// DIMTDEC - Tolerance decimal places
    pub dim_tolerance_decimal_places: i16,
    /// DIMALTU - Alternate units format
    pub dim_alt_units_format: i16,
    /// DIMALTTD - Alternate tolerance decimal places
    pub dim_alt_tolerance_decimal_places: i16,
    /// DIMAUNIT - Angular units format
    pub dim_angular_units: i16,
    /// DIMFRAC - Fraction format
    pub dim_fraction_format: i16,
    /// DIMLUNIT - Linear unit format
    pub dim_linear_unit_format: i16,
    /// DIMDSEP - Decimal separator
    pub dim_decimal_separator: char,
    /// DIMTMOVE - Text movement
    pub dim_text_movement: i16,
    /// DIMJUST - Horizontal text justification
    pub dim_horizontal_justification: i16,
    /// DIMSD1 - Suppress dimension line 1
    pub dim_suppress_line1: bool,
    /// DIMSD2 - Suppress dimension line 2
    pub dim_suppress_line2: bool,
    /// DIMTOLJ - Tolerance vertical justification
    pub dim_tolerance_justification: i16,
    /// DIMTZIN - Tolerance zero suppression
    pub dim_tolerance_zero_suppression: i16,
    /// DIMALTZ - Alternate tolerance zero suppression
    pub dim_alt_tolerance_zero_suppression: i16,
    /// DIMALTTZ - Alternate tolerance zero suppression (tight)
    pub dim_alt_tolerance_zero_tight: i16,
    /// DIMFIT/DIMATFIT - Fit options
    pub dim_fit: i16,
    /// DIMUPT - User positioned text
    pub dim_user_positioned_text: bool,
    /// DIMPOST - Primary units suffix
    pub dim_post: String,
    /// DIMAPOST - Alternate units suffix
    pub dim_alt_post: String,
    /// DIMBLK - Arrow block name
    pub dim_arrow_block: String,
    /// DIMBLK1 - First arrow block name
    pub dim_arrow_block1: String,
    /// DIMBLK2 - Second arrow block name
    pub dim_arrow_block2: String,
    /// DIMLDRBLK - Leader arrow block name
    pub dim_leader_arrow_block: String,
    
    // ==================== Extents and Limits ====================
    /// INSBASE - Model space insertion base point
    pub model_space_insertion_base: Vector3,
    /// EXTMIN - Model space extents min
    pub model_space_extents_min: Vector3,
    /// EXTMAX - Model space extents max
    pub model_space_extents_max: Vector3,
    /// LIMMIN - Model space limits min
    pub model_space_limits_min: Vector2,
    /// LIMMAX - Model space limits max
    pub model_space_limits_max: Vector2,
    
    /// Paper space insertion base point
    pub paper_space_insertion_base: Vector3,
    /// Paper space extents min
    pub paper_space_extents_min: Vector3,
    /// Paper space extents max
    pub paper_space_extents_max: Vector3,
    /// Paper space limits min
    pub paper_space_limits_min: Vector2,
    /// Paper space limits max
    pub paper_space_limits_max: Vector2,
    
    // ==================== UCS Settings ====================
    /// UCSBASE - UCS base name
    pub ucs_base: String,
    /// Model space UCS name
    pub model_space_ucs_name: String,
    /// Paper space UCS name  
    pub paper_space_ucs_name: String,
    /// Model space UCS origin
    pub model_space_ucs_origin: Vector3,
    /// Model space UCS X axis
    pub model_space_ucs_x_axis: Vector3,
    /// Model space UCS Y axis
    pub model_space_ucs_y_axis: Vector3,
    /// Paper space UCS origin
    pub paper_space_ucs_origin: Vector3,
    /// Paper space UCS X axis
    pub paper_space_ucs_x_axis: Vector3,
    /// Paper space UCS Y axis
    pub paper_space_ucs_y_axis: Vector3,
    /// UCSORTHOREF - UCS orthographic reference
    pub ucs_ortho_ref: Handle,
    /// UCSORTHOVIEW - UCS orthographic view type
    pub ucs_ortho_view: i16,
    /// PUCSORTHOREF - Paper space UCS orthographic reference  
    pub paper_ucs_ortho_ref: Handle,
    /// PUCSORTHOVIEW - Paper space UCS orthographic view type
    pub paper_ucs_ortho_view: i16,
    
    // ==================== Handles/References ====================
    /// HANDSEED - Next available handle
    pub handle_seed: u64,
    /// Current layer handle
    pub current_layer_handle: Handle,
    /// Current text style handle
    pub current_text_style_handle: Handle,
    /// Current linetype handle
    pub current_linetype_handle: Handle,
    /// Current dimension style handle
    pub current_dimstyle_handle: Handle,
    /// Current multiline style handle
    pub current_multiline_style_handle: Handle,
    /// Current material handle
    pub current_material_handle: Handle,
    /// Dimension text style handle
    pub dim_text_style_handle: Handle,
    /// Dimension linetype handle
    pub dim_linetype_handle: Handle,
    /// Dimension linetype 1 handle
    pub dim_linetype1_handle: Handle,
    /// Dimension linetype 2 handle
    pub dim_linetype2_handle: Handle,
    /// Dimension arrow block handle
    pub dim_arrow_block_handle: Handle,
    /// Dimension arrow block 1 handle
    pub dim_arrow_block1_handle: Handle,
    /// Dimension arrow block 2 handle
    pub dim_arrow_block2_handle: Handle,
    /// DIMLWD - Dimension line weight
    pub dim_line_weight: i16,
    /// DIMLWE - Extension line weight
    pub dim_ext_line_weight: i16,

    // ==================== Table Control Object Handles ====================
    /// Block table control object
    pub block_control_handle: Handle,
    /// Layer table control object
    pub layer_control_handle: Handle,
    /// Text style table control object
    pub style_control_handle: Handle,
    /// Linetype table control object
    pub linetype_control_handle: Handle,
    /// View table control object
    pub view_control_handle: Handle,
    /// UCS table control object
    pub ucs_control_handle: Handle,
    /// Viewport table control object
    pub vport_control_handle: Handle,
    /// AppId table control object
    pub appid_control_handle: Handle,
    /// Dimension style table control object
    pub dimstyle_control_handle: Handle,
    /// VPEntHdr table control object
    pub vpent_hdr_control_handle: Handle,
    
    // ==================== Dictionary Handles ====================
    /// Named objects dictionary
    pub named_objects_dict_handle: Handle,
    /// ACAD_GROUP dictionary
    pub acad_group_dict_handle: Handle,
    /// ACAD_MLINESTYLE dictionary
    pub acad_mlinestyle_dict_handle: Handle,
    /// ACAD_LAYOUT dictionary (R2000+)
    pub acad_layout_dict_handle: Handle,
    /// ACAD_PLOTSETTINGS dictionary (R2000+)
    pub acad_plotsettings_dict_handle: Handle,
    /// ACAD_PLOTSTYLENAME dictionary (R2000+)
    pub acad_plotstylename_dict_handle: Handle,
    /// ACAD_MATERIAL dictionary (R2007+)
    pub acad_material_dict_handle: Handle,
    /// ACAD_COLOR dictionary (R2007+)
    pub acad_color_dict_handle: Handle,
    /// ACAD_VISUALSTYLE dictionary (R2007+)
    pub acad_visualstyle_dict_handle: Handle,
    
    // ==================== Block Record Handles ====================
    /// *MODEL_SPACE block record
    pub model_space_block_handle: Handle,
    /// *PAPER_SPACE block record
    pub paper_space_block_handle: Handle,
    /// BYLAYER linetype
    pub bylayer_linetype_handle: Handle,
    /// BYBLOCK linetype
    pub byblock_linetype_handle: Handle,
    /// CONTINUOUS linetype
    pub continuous_linetype_handle: Handle,
    
    // ==================== Date/Time ====================
    /// Document creation time (Julian date)
    pub create_date_julian: f64,
    /// Document update time (Julian date)
    pub update_date_julian: f64,
    /// Total editing time in days
    pub total_editing_time: f64,
    /// User elapsed time in days
    pub user_elapsed_time: f64,
    
    // ==================== Metadata ====================
    /// Fingerprint GUID
    pub fingerprint_guid: String,
    /// Version GUID
    pub version_guid: String,
    /// Menu file name
    pub menu_name: String,
    /// DWGCODEPAGE
    pub code_page: String,
    /// LASTSAVEDBY
    pub last_saved_by: String,
    /// HYPERLINKBASE
    pub hyperlink_base: String,
    /// STYLESHEET
    pub stylesheet: String,
    
    // ==================== Misc ====================
    /// MEASUREMENT - Drawing units (0=English, 1=Metric)
    pub measurement: i16,
    /// PROXYGRAPHICS - Show proxy graphics
    pub proxy_graphics: i16,
    /// TREEDEPTH - Tree depth for spatial index
    pub tree_depth: i16,
    /// CMLSTYLE - Current multiline style name
    pub multiline_style: String,
    /// CELTYPE - Current linetype name
    pub current_linetype_name: String,
    /// CLAYER - Current layer name
    pub current_layer_name: String,
    /// TEXTSTYLE - Current text style name
    pub current_text_style_name: String,
    /// DIMSTYLE - Current dimension style name
    pub current_dimstyle_name: String,
    /// CTABLESTYLE - Current table style name
    pub current_table_style_name: String,
    /// CMLEADERSTYLE - Current multileader style name
    pub current_mleader_style_name: String,
}

impl Default for HeaderVariables {
    fn default() -> Self {
        Self {
            // Version-specific flags
            required_versions: 0,
            
            // Drawing mode flags
            associate_dimensions: true,
            update_dimensions_while_dragging: true,
            ortho_mode: false,
            fill_mode: true,
            quick_text_mode: false,
            mirror_text: false,
            regen_mode: true,
            limit_check: false,
            paper_space_limit_check: false,
            polyline_linetype_generation: false,
            paper_space_linetype_scaling: true,
            show_model_space: true,
            user_timer: true,
            world_view: true,
            retain_xref_visibility: true,
            display_silhouette: false,
            spline_frame: false,
            delete_objects: true,
            drag_mode: 2,
            blip_mode: false,
            attribute_request: true,
            attribute_dialog: true,
            
            // Unit settings
            linear_unit_format: 2,  // Decimal
            linear_unit_precision: 4,
            angular_unit_format: 0, // Decimal degrees
            angular_unit_precision: 0,
            insertion_units: 0,     // Unitless
            attribute_visibility: 1,
            point_display_mode: 0,
            user_int1: 0, user_int2: 0, user_int3: 0, user_int4: 0, user_int5: 0,
            coords_mode: 2,
            object_snap_mode: 0,
            pick_style: 1,
            spline_type: 6,
            spline_segments: 8,
            spline_segs_surface: 6,
            surface_u_density: 6,
            surface_v_density: 6,
            surface_type: 6,
            surface_tab1: 6,
            surface_tab2: 6,
            shade_edge: 3,
            shade_diffuse: 70,
            max_active_viewports: 64,
            isolines: 4,
            multiline_justification: 0,
            text_quality: 50,
            sort_entities: 127,
            index_control: 0,
            hide_text: 1,
            xclip_frame: 0,
            halo_gap: 0,
            obscured_color: 257,
            obscured_linetype: 0,
            intersection_display: 0,
            intersection_color: 257,
            dimension_associativity: 2,
            project_name: String::new(),
            
            // Scale/size defaults
            linetype_scale: 1.0,
            text_height: 2.5,
            trace_width: 0.05,
            sketch_increment: 0.1,
            thickness: 0.0,
            point_display_size: 0.0,
            polyline_width: 0.0,
            current_entity_linetype_scale: 1.0,
            view_twist: 0.0,
            fillet_radius: 0.0,
            chamfer_distance_a: 0.0,
            chamfer_distance_b: 0.0,
            chamfer_length: 0.0,
            chamfer_angle: 0.0,
            angle_base: 0.0,
            angle_direction: 0,
            elevation: 0.0,
            paper_elevation: 0.0,
            facet_resolution: 0.5,
            multiline_scale: 1.0,
            user_real1: 0.0, user_real2: 0.0, user_real3: 0.0, user_real4: 0.0, user_real5: 0.0,
            viewport_scale_factor: 0.0,
            current_annotation_scale: "1:1".to_string(),
            annotation_scale_value: 1.0,
            shadow_plane_location: 0.0,
            loft_angle1: std::f64::consts::FRAC_PI_2,
            loft_angle2: std::f64::consts::FRAC_PI_2,
            loft_magnitude1: 0.0,
            loft_magnitude2: 0.0,
            loft_param: 7,
            loft_normals: 1,
            latitude: 37.795,
            longitude: -122.394,
            north_direction: 0.0,
            timezone: -8000,
            steps_per_second: 2.0,
            step_size: 6.0,
            lens_length: 50.0,
            camera_height: 0.0,
            camera_display: false,
            
            // Current entity settings
            current_entity_color: Color::ByLayer,
            current_line_weight: -1, // ByLayer
            current_plotstyle_type: 0,
            end_caps: 0,
            join_style: 0,
            lineweight_display: false,
            xedit: true,
            extended_names: true,
            plotstyle_mode: true,
            ole_startup: false,
            
            // Dimension variables
            dim_scale: 1.0,
            dim_arrow_size: 0.18,
            dim_ext_line_offset: 0.0625,
            dim_line_increment: 0.38,
            dim_ext_line_extension: 0.18,
            dim_rounding: 0.0,
            dim_line_extension: 0.0,
            dim_tolerance_plus: 0.0,
            dim_tolerance_minus: 0.0,
            dim_text_height: 0.18,
            dim_center_mark: 0.09,
            dim_tick_size: 0.0,
            dim_alt_scale: 25.4,
            dim_linear_scale: 1.0,
            dim_text_vertical_pos: 0.0,
            dim_tolerance_scale: 1.0,
            dim_line_gap: 0.09,
            dim_alt_rounding: 0.0,
            dim_tolerance: false,
            dim_limits: false,
            dim_text_inside_horizontal: true,
            dim_text_outside_horizontal: true,
            dim_suppress_ext1: false,
            dim_suppress_ext2: false,
            dim_text_above: 0,
            dim_zero_suppression: 0,
            dim_alt_zero_suppression: 0,
            dim_alternate_units: false,
            dim_alt_decimal_places: 2,
            dim_force_line_inside: false,
            dim_separate_arrows: false,
            dim_force_text_inside: false,
            dim_suppress_outside_ext: false,
            dim_line_color: Color::ByBlock,
            dim_ext_line_color: Color::ByBlock,
            dim_text_color: Color::ByBlock,
            dim_angular_decimal_places: 0,
            dim_decimal_places: 4,
            dim_tolerance_decimal_places: 4,
            dim_alt_units_format: 2,
            dim_alt_tolerance_decimal_places: 4,
            dim_angular_units: 0,
            dim_fraction_format: 0,
            dim_linear_unit_format: 2,
            dim_decimal_separator: '.',
            dim_text_movement: 0,
            dim_horizontal_justification: 0,
            dim_suppress_line1: false,
            dim_suppress_line2: false,
            dim_tolerance_justification: 1,
            dim_tolerance_zero_suppression: 0,
            dim_alt_tolerance_zero_suppression: 0,
            dim_alt_tolerance_zero_tight: 0,
            dim_fit: 3,
            dim_user_positioned_text: false,
            dim_post: String::new(),
            dim_alt_post: String::new(),
            dim_arrow_block: String::new(),
            dim_arrow_block1: String::new(),
            dim_arrow_block2: String::new(),
            dim_leader_arrow_block: String::new(),
            
            // Extents and limits - Model space
            model_space_insertion_base: Vector3::ZERO,
            model_space_extents_min: Vector3::new(1e20, 1e20, 1e20),
            model_space_extents_max: Vector3::new(-1e20, -1e20, -1e20),
            model_space_limits_min: Vector2::new(0.0, 0.0),
            model_space_limits_max: Vector2::new(12.0, 9.0),
            
            // Extents and limits - Paper space
            paper_space_insertion_base: Vector3::ZERO,
            paper_space_extents_min: Vector3::new(1e20, 1e20, 1e20),
            paper_space_extents_max: Vector3::new(-1e20, -1e20, -1e20),
            paper_space_limits_min: Vector2::new(0.0, 0.0),
            paper_space_limits_max: Vector2::new(12.0, 9.0),
            
            // UCS settings
            ucs_base: String::new(),
            model_space_ucs_name: String::new(),
            paper_space_ucs_name: String::new(),
            model_space_ucs_origin: Vector3::ZERO,
            model_space_ucs_x_axis: Vector3::new(1.0, 0.0, 0.0),
            model_space_ucs_y_axis: Vector3::new(0.0, 1.0, 0.0),
            paper_space_ucs_origin: Vector3::ZERO,
            paper_space_ucs_x_axis: Vector3::new(1.0, 0.0, 0.0),
            paper_space_ucs_y_axis: Vector3::new(0.0, 1.0, 0.0),
            ucs_ortho_ref: Handle::NULL,
            ucs_ortho_view: 0,
            paper_ucs_ortho_ref: Handle::NULL,
            paper_ucs_ortho_view: 0,
            
            // Handles
            handle_seed: 1,
            current_layer_handle: Handle::NULL,
            current_text_style_handle: Handle::NULL,
            current_linetype_handle: Handle::NULL,
            current_dimstyle_handle: Handle::NULL,
            current_multiline_style_handle: Handle::NULL,
            current_material_handle: Handle::NULL,
            dim_text_style_handle: Handle::NULL,
            dim_linetype_handle: Handle::NULL,
            dim_linetype1_handle: Handle::NULL,
            dim_linetype2_handle: Handle::NULL,
            dim_arrow_block_handle: Handle::NULL,
            dim_arrow_block1_handle: Handle::NULL,
            dim_arrow_block2_handle: Handle::NULL,
            dim_line_weight: -2,      // ByBlock
            dim_ext_line_weight: -2,  // ByBlock
            
            // Table control handles
            block_control_handle: Handle::NULL,
            layer_control_handle: Handle::NULL,
            style_control_handle: Handle::NULL,
            linetype_control_handle: Handle::NULL,
            view_control_handle: Handle::NULL,
            ucs_control_handle: Handle::NULL,
            vport_control_handle: Handle::NULL,
            appid_control_handle: Handle::NULL,
            dimstyle_control_handle: Handle::NULL,
            vpent_hdr_control_handle: Handle::NULL,
            
            // Dictionary handles
            named_objects_dict_handle: Handle::NULL,
            acad_group_dict_handle: Handle::NULL,
            acad_mlinestyle_dict_handle: Handle::NULL,
            acad_layout_dict_handle: Handle::NULL,
            acad_plotsettings_dict_handle: Handle::NULL,
            acad_plotstylename_dict_handle: Handle::NULL,
            acad_material_dict_handle: Handle::NULL,
            acad_color_dict_handle: Handle::NULL,
            acad_visualstyle_dict_handle: Handle::NULL,
            
            // Block record handles
            model_space_block_handle: Handle::NULL,
            paper_space_block_handle: Handle::NULL,
            bylayer_linetype_handle: Handle::NULL,
            byblock_linetype_handle: Handle::NULL,
            continuous_linetype_handle: Handle::NULL,
            
            // Date/time
            create_date_julian: 0.0,
            update_date_julian: 0.0,
            total_editing_time: 0.0,
            user_elapsed_time: 0.0,
            
            // Metadata
            fingerprint_guid: String::new(),
            version_guid: String::new(),
            menu_name: String::new(),
            code_page: String::from("ANSI_1252"),
            last_saved_by: String::new(),
            hyperlink_base: String::new(),
            stylesheet: String::new(),
            
            // Misc
            measurement: 0,
            proxy_graphics: 1,
            tree_depth: 3020,
            multiline_style: String::from("Standard"),
            current_linetype_name: String::from("ByLayer"),
            current_layer_name: String::from("0"),
            current_text_style_name: String::from("Standard"),
            current_dimstyle_name: String::from("Standard"),
            current_table_style_name: String::from("Standard"),
            current_mleader_style_name: String::from("Standard"),
        }
    }
}

/// A CAD document containing all drawing data
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CadDocument {
    /// Document version
    pub version: DxfVersion,
    
    /// AutoCAD maintenance release version (from file header byte 0x0B).
    ///
    /// Used to determine encoding variations within a major DWG version.
    /// For AC1024 (R2010), maintenance > 3 triggers an extra 4-byte RL field
    /// in the Classes and Header sections.  Preserved during roundtrip.
    pub maintenance_version: u8,
    
    /// Header variables containing drawing settings
    pub header: HeaderVariables,
    
    /// Layer table
    pub layers: Table<Layer>,
    
    /// Line type table
    pub line_types: Table<LineType>,
    
    /// Text style table
    pub text_styles: Table<TextStyle>,
    
    /// Block record table
    pub block_records: Table<BlockRecord>,
    
    /// Dimension style table
    pub dim_styles: Table<DimStyle>,
    
    /// Application ID table
    pub app_ids: Table<AppId>,
    
    /// View table
    pub views: Table<View>,
    
    /// Viewport table
    pub vports: Table<VPort>,
    
    /// UCS table
    pub ucss: Table<Ucs>,
    
    /// DXF class definitions (CLASSES section)
    pub classes: DxfClassCollection,

    /// Notifications collected during the last read/write operation
    pub notifications: crate::notification::NotificationCollection,

    /// All entities in the document (contiguous storage for cache locality).
    pub(crate) entities: Vec<EntityType>,

    /// Handle → index mapping for O(1) entity lookup by handle.
    pub(crate) entity_index: HashMap<Handle, usize>,

    /// All objects in the document (indexed by handle)
    pub objects: HashMap<Handle, ObjectType>,

    /// Raw EED blobs per handle — populated during DWG read, consumed during DWG write.
    /// Keyed by the object/table-entry handle. Not serialized.
    pub(crate) eed_by_handle: HashMap<Handle, Vec<(u64, Vec<u8>)>>,

    /// Non-entity object xdictionary handles — populated during DWG read, consumed during DWG write.
    pub(crate) xdic_by_handle: HashMap<Handle, Handle>,

    /// Non-entity object reactors — populated during DWG read, consumed during DWG write.
    pub(crate) reactors_by_handle: HashMap<Handle, Vec<Handle>>,

    /// Original BLOCK_HEADER entity handles from the DWG binary — includes sub-entity handles
    /// (vertices, faces, SEQENDs). Keyed by BlockRecord handle. Used by the writer to produce
    /// correct owned_object_count without re-expanding from the document model.
    pub(crate) block_entity_handles: HashMap<Handle, Vec<Handle>>,

    /// Next handle to assign
    next_handle: u64,
}

impl CadDocument {
    /// Create a new empty CAD document
    pub fn new() -> Self {
        let mut doc = CadDocument {
            version: DxfVersion::AC1032, // DXF 2018
            maintenance_version: 0,
            header: HeaderVariables::default(),
            layers: Table::new(),
            line_types: Table::new(),
            text_styles: Table::new(),
            block_records: Table::new(),
            dim_styles: Table::new(),
            app_ids: Table::new(),
            views: Table::new(),
            vports: Table::new(),
            ucss: Table::new(),
            classes: DxfClassCollection::new(),
            notifications: crate::notification::NotificationCollection::new(),
            entities: Vec::new(),
            entity_index: HashMap::new(),
            objects: HashMap::new(),
            eed_by_handle: HashMap::new(),
            xdic_by_handle: HashMap::new(),
            reactors_by_handle: HashMap::new(),
            block_entity_handles: HashMap::new(),
            // Start handle allocation above reserved table handles (0x1-0xA)
            // Table handles are well-known fixed values used by AutoCAD
            next_handle: 0x10,
        };
        
        // Initialize with standard entries
        doc.initialize_defaults();
        doc
    }

    /// Create a document with a specific version
    pub fn with_version(version: DxfVersion) -> Self {
        let mut doc = Self::new();
        doc.version = version;
        doc
    }

    /// Initialize default tables with standard entries
    fn initialize_defaults(&mut self) {
        // Allocate table control handles first (these are well-known handles in DWG)
        self.header.block_control_handle = Handle::new(0x01);
        self.header.layer_control_handle = Handle::new(0x02);
        self.header.style_control_handle = Handle::new(0x03);
        self.header.linetype_control_handle = Handle::new(0x05);
        self.header.view_control_handle = Handle::new(0x06);
        self.header.ucs_control_handle = Handle::new(0x07);
        self.header.vport_control_handle = Handle::new(0x08);
        self.header.appid_control_handle = Handle::new(0x09);
        self.header.dimstyle_control_handle = Handle::new(0x0A);
        self.header.vpent_hdr_control_handle = Handle::new(0x0B);
        self.header.named_objects_dict_handle = Handle::new(0x0C);

        // Assign allocated table control handles TO the Table objects so the
        // object writer uses the same handles the header section references.
        // Without this, Table<T>.handle() returns Handle::NULL and every
        // table control is written with handle 0, not registered in the
        // handle map, and unreachable by readers → "invalid data" for all objects.
        self.block_records.set_handle(self.header.block_control_handle);
        self.layers.set_handle(self.header.layer_control_handle);
        self.text_styles.set_handle(self.header.style_control_handle);
        self.line_types.set_handle(self.header.linetype_control_handle);
        self.views.set_handle(self.header.view_control_handle);
        self.ucss.set_handle(self.header.ucs_control_handle);
        self.vports.set_handle(self.header.vport_control_handle);
        self.app_ids.set_handle(self.header.appid_control_handle);
        self.dim_styles.set_handle(self.header.dimstyle_control_handle);

        // Add standard layer "0"
        let mut layer0 = Layer::layer_0();
        layer0.set_handle(self.allocate_handle());
        // Store the layer handle for CLAYER
        self.header.current_layer_handle = layer0.handle;
        self.layers.add(layer0).ok();

        // Add standard line types
        let mut continuous = LineType::continuous();
        continuous.set_handle(self.allocate_handle());
        self.header.continuous_linetype_handle = continuous.handle;
        self.line_types.add(continuous).ok();

        let mut by_layer = LineType::by_layer();
        by_layer.set_handle(self.allocate_handle());
        self.header.bylayer_linetype_handle = by_layer.handle;
        self.header.current_linetype_handle = by_layer.handle; // Default linetype is ByLayer
        self.line_types.add(by_layer).ok();

        let mut by_block = LineType::by_block();
        by_block.set_handle(self.allocate_handle());
        self.header.byblock_linetype_handle = by_block.handle;
        self.line_types.add(by_block).ok();

        // Add standard text style
        let mut standard_style = TextStyle::standard();
        standard_style.set_handle(self.allocate_handle());
        self.header.current_text_style_handle = standard_style.handle;
        self.text_styles.add(standard_style).ok();

        // Add model space and paper space blocks
        let mut model_space = BlockRecord::model_space();
        model_space.set_handle(self.allocate_handle());
        model_space.block_entity_handle = self.allocate_handle();
        model_space.block_end_handle = self.allocate_handle();
        self.header.model_space_block_handle = model_space.handle;
        self.block_records.add(model_space).ok();

        let mut paper_space = BlockRecord::paper_space();
        paper_space.set_handle(self.allocate_handle());
        paper_space.block_entity_handle = self.allocate_handle();
        paper_space.block_end_handle = self.allocate_handle();
        self.header.paper_space_block_handle = paper_space.handle;
        self.block_records.add(paper_space).ok();

        // Add standard dimension style
        let mut standard_dimstyle = DimStyle::standard();
        standard_dimstyle.set_handle(self.allocate_handle());
        // DIMTXSTY must reference the Standard text style
        standard_dimstyle.dimtxsty_handle = self.header.current_text_style_handle;
        self.header.current_dimstyle_handle = standard_dimstyle.handle;
        // Header dim text style handle must also point to Standard
        self.header.dim_text_style_handle = self.header.current_text_style_handle;
        // Dim linetype handles: reference ByBlock linetype for R2007+
        self.header.dim_linetype_handle = self.header.byblock_linetype_handle;
        self.header.dim_linetype1_handle = self.header.byblock_linetype_handle;
        self.header.dim_linetype2_handle = self.header.byblock_linetype_handle;
        self.dim_styles.add(standard_dimstyle).ok();

        // Add standard application ID
        let mut acad = AppId::acad();
        acad.set_handle(self.allocate_handle());
        self.app_ids.add(acad).ok();

        // Application ID under which annotative styles store their flag (XDATA).
        let mut annotative = AppId::new("AcadAnnotative");
        annotative.set_handle(self.allocate_handle());
        self.app_ids.add(annotative).ok();

        // Add standard viewport
        let mut active_vport = VPort::active();
        active_vport.set_handle(self.allocate_handle());
        self.vports.add(active_vport).ok();
        
        // ── Standard dictionary objects (required for DWG format) ────
        // Allocate handles for core dictionaries
        self.header.acad_group_dict_handle = self.allocate_handle();
        self.header.acad_mlinestyle_dict_handle = self.allocate_handle();
        self.header.acad_layout_dict_handle = self.allocate_handle();
        self.header.acad_plotsettings_dict_handle = self.allocate_handle();
        self.header.acad_plotstylename_dict_handle = self.allocate_handle();
        // R2004+/R2007+ dictionaries (AutoCAD requires these even if empty)
        self.header.acad_material_dict_handle = self.allocate_handle();
        self.header.acad_color_dict_handle = self.allocate_handle();
        self.header.acad_visualstyle_dict_handle = self.allocate_handle();

        // Allocate handles for objects that live inside dictionaries
        let mlinestyle_std_handle = self.allocate_handle();
        let model_layout_handle = self.allocate_handle();
        let paper_layout_handle = self.allocate_handle();
        let plotstylename_placeholder_handle = self.allocate_handle();

        // Store the current MLineStyle handle in the header (for CMLSTYLE)
        self.header.current_multiline_style_handle = mlinestyle_std_handle;

        // Link block records to their layouts
        if let Some(ms) = self.block_records.get_mut("*Model_Space") {
            ms.layout = model_layout_handle;
        }
        if let Some(ps) = self.block_records.get_mut("*Paper_Space") {
            ps.layout = paper_layout_handle;
        }

        // -- Root dictionary (NAMED_OBJECTS_DICTIONARY) --
        let root_dict_handle = self.header.named_objects_dict_handle;
        let mut root_dict = crate::objects::Dictionary::new();
        root_dict.handle = root_dict_handle;
        root_dict.owner = Handle::NULL; // owned by document
        root_dict.add_entry("ACAD_GROUP", self.header.acad_group_dict_handle);
        root_dict.add_entry("ACAD_MLINESTYLE", self.header.acad_mlinestyle_dict_handle);
        root_dict.add_entry("ACAD_LAYOUT", self.header.acad_layout_dict_handle);
        root_dict.add_entry("ACAD_PLOTSETTINGS", self.header.acad_plotsettings_dict_handle);
        root_dict.add_entry("ACAD_PLOTSTYLENAME", self.header.acad_plotstylename_dict_handle);
        root_dict.add_entry("ACAD_MATERIAL", self.header.acad_material_dict_handle);
        root_dict.add_entry("ACAD_COLOR", self.header.acad_color_dict_handle);
        root_dict.add_entry("ACAD_VISUALSTYLE", self.header.acad_visualstyle_dict_handle);
        self.objects.insert(root_dict_handle, ObjectType::Dictionary(root_dict));

        // -- ACAD_GROUP dictionary (empty) --
        let mut group_dict = crate::objects::Dictionary::new();
        group_dict.handle = self.header.acad_group_dict_handle;
        group_dict.owner = root_dict_handle;
        self.objects.insert(group_dict.handle, ObjectType::Dictionary(group_dict));

        // -- ACAD_MLINESTYLE dictionary (contains "Standard") --
        let mut mlinestyle_dict = crate::objects::Dictionary::new();
        mlinestyle_dict.handle = self.header.acad_mlinestyle_dict_handle;
        mlinestyle_dict.owner = root_dict_handle;
        mlinestyle_dict.add_entry("Standard", mlinestyle_std_handle);
        self.objects.insert(mlinestyle_dict.handle, ObjectType::Dictionary(mlinestyle_dict));

        // -- MLineStyle Standard object --
        let mut mlinestyle_std = crate::objects::MLineStyle::standard();
        mlinestyle_std.handle = mlinestyle_std_handle;
        mlinestyle_std.owner = self.header.acad_mlinestyle_dict_handle;
        self.objects.insert(mlinestyle_std_handle, ObjectType::MLineStyle(mlinestyle_std));

        // -- ACAD_LAYOUT dictionary (Model + Layout1) --
        let mut layout_dict = crate::objects::Dictionary::new();
        layout_dict.handle = self.header.acad_layout_dict_handle;
        layout_dict.owner = root_dict_handle;
        layout_dict.add_entry("Model", model_layout_handle);
        layout_dict.add_entry("Layout1", paper_layout_handle);
        self.objects.insert(layout_dict.handle, ObjectType::Dictionary(layout_dict));

        // -- Layout: Model --
        let mut model_layout = crate::objects::Layout::new("Model");
        model_layout.handle = model_layout_handle;
        model_layout.owner = self.header.acad_layout_dict_handle;
        model_layout.tab_order = 0;
        model_layout.flags = 1; // model space
        model_layout.block_record = self.header.model_space_block_handle;
        self.objects.insert(model_layout_handle, ObjectType::Layout(model_layout));

        // -- Layout: Layout1 (paper space) --
        let mut paper_layout = crate::objects::Layout::new("Layout1");
        paper_layout.handle = paper_layout_handle;
        paper_layout.owner = self.header.acad_layout_dict_handle;
        paper_layout.tab_order = 1;
        paper_layout.block_record = self.header.paper_space_block_handle;

        self.objects.insert(paper_layout_handle, ObjectType::Layout(paper_layout));

        // -- ACAD_PLOTSETTINGS dictionary (empty) --
        let mut plotsettings_dict = crate::objects::Dictionary::new();
        plotsettings_dict.handle = self.header.acad_plotsettings_dict_handle;
        plotsettings_dict.owner = root_dict_handle;
        self.objects.insert(plotsettings_dict.handle, ObjectType::Dictionary(plotsettings_dict));

        // -- ACAD_MATERIAL dictionary (empty, required R2004+) --
        let mut material_dict = crate::objects::Dictionary::new();
        material_dict.handle = self.header.acad_material_dict_handle;
        material_dict.owner = root_dict_handle;
        self.objects.insert(material_dict.handle, ObjectType::Dictionary(material_dict));

        // -- ACAD_COLOR dictionary (empty, required R2004+) --
        let mut color_dict = crate::objects::Dictionary::new();
        color_dict.handle = self.header.acad_color_dict_handle;
        color_dict.owner = root_dict_handle;
        self.objects.insert(color_dict.handle, ObjectType::Dictionary(color_dict));

        // -- ACAD_VISUALSTYLE dictionary (empty, required R2007+) --
        let mut visualstyle_dict = crate::objects::Dictionary::new();
        visualstyle_dict.handle = self.header.acad_visualstyle_dict_handle;
        visualstyle_dict.owner = root_dict_handle;
        self.objects.insert(visualstyle_dict.handle, ObjectType::Dictionary(visualstyle_dict));

        // -- ACAD_PLOTSTYLENAME dictionary (DictionaryWithDefault with PlaceHolder) --
        let mut plotstyle_dict = crate::objects::DictionaryWithDefault::new();
        plotstyle_dict.handle = self.header.acad_plotstylename_dict_handle;
        plotstyle_dict.owner = root_dict_handle;
        plotstyle_dict.default_handle = plotstylename_placeholder_handle;
        plotstyle_dict.entries.push(("Normal".to_string(), plotstylename_placeholder_handle));
        self.objects.insert(plotstyle_dict.handle, ObjectType::DictionaryWithDefault(plotstyle_dict));

        // -- PlaceHolder for ACAD_PLOTSTYLENAME "Normal" --
        let mut placeholder = crate::objects::PlaceHolder::new();
        placeholder.handle = plotstylename_placeholder_handle;
        placeholder.owner = self.header.acad_plotstylename_dict_handle;
        self.objects.insert(plotstylename_placeholder_handle, ObjectType::PlaceHolder(placeholder));

        // Register standard DXF classes required by the DWG format.
        // For pre-R2004, "unlisted" object types (LAYOUT, PLOTSETTINGS, etc.)
        // need a class entry so the writer can emit the class number instead of
        // the R2004+ fixed type code.
        use crate::classes::{DxfClass, ProxyFlags};
        let standard_classes = [
            DxfClass {
                dxf_name: "ACDBDICTIONARYWDFLT".to_string(),
                cpp_class_name: "AcDbDictionaryWithDefault".to_string(),
                application_name: "ObjectDBX Classes".to_string(),
                proxy_flags: ProxyFlags::NONE,
                instance_count: 0,
                was_zombie: false,
                is_an_entity: false,
                class_number: 0, // will be assigned (500+)
                item_class_id: 0x1F3,
            },
            DxfClass {
                dxf_name: "DICTIONARYVAR".to_string(),
                cpp_class_name: "AcDbDictionaryVar".to_string(),
                application_name: "ObjectDBX Classes".to_string(),
                proxy_flags: ProxyFlags::NONE,
                instance_count: 0,
                was_zombie: false,
                is_an_entity: false,
                class_number: 0,
                item_class_id: 0x1F3,
            },
            DxfClass {
                dxf_name: "LAYOUT".to_string(),
                cpp_class_name: "AcDbLayout".to_string(),
                application_name: "ObjectDBX Classes".to_string(),
                proxy_flags: ProxyFlags::NONE,
                instance_count: 0,
                was_zombie: false,
                is_an_entity: false,
                class_number: 0,
                item_class_id: 0x1F3,
            },
            DxfClass {
                dxf_name: "ACDBPLACEHOLDER".to_string(),
                cpp_class_name: "AcDbPlaceHolder".to_string(),
                application_name: "ObjectDBX Classes".to_string(),
                proxy_flags: ProxyFlags::NONE,
                instance_count: 0,
                was_zombie: false,
                is_an_entity: false,
                class_number: 0,
                item_class_id: 0x1F3,
            },
            DxfClass {
                dxf_name: "PLOTSETTINGS".to_string(),
                cpp_class_name: "AcDbPlotSettings".to_string(),
                application_name: "ObjectDBX Classes".to_string(),
                proxy_flags: ProxyFlags::NONE,
                instance_count: 0,
                was_zombie: false,
                is_an_entity: false,
                class_number: 0,
                item_class_id: 0x1F3,
            },
            DxfClass {
                dxf_name: "SCALE".to_string(),
                cpp_class_name: "AcDbScale".to_string(),
                application_name: "ObjectDBX Classes".to_string(),
                proxy_flags: ProxyFlags::NONE,
                instance_count: 0,
                was_zombie: false,
                is_an_entity: false,
                class_number: 0,
                item_class_id: 0x1F3,
            },
        ];
        for cls in standard_classes {
            self.classes.add_or_update(cls);
        }

        // Register default DXF classes for all entity/object types.
        // Unlisted types like MESH, MULTILEADER, IMAGE need class entries
        // so the writer emits the correct 500+ type code instead of a
        // wrong fixed code.
        self.classes.update_defaults();
    }

    /// Allocate a new unique handle
    pub fn allocate_handle(&mut self) -> Handle {
        let handle = Handle::new(self.next_handle);
        self.next_handle += 1;
        // Keep HANDSEED in sync — DWG header requires this to be ≥ next_handle
        self.header.handle_seed = self.next_handle;
        handle
    }

    /// Get the next handle value (without allocating)
    pub fn next_handle(&self) -> u64 {
        self.next_handle
    }

    /// Add an entity to the document (model space).
    ///
    /// The entity is stored in both the flat entity map (used by the DXF
    /// writer) and the *Model_Space block record (used by the DWG writer).
    pub fn add_entity(&mut self, mut entity: EntityType) -> Result<Handle> {
        // Allocate a handle if the entity doesn't have one
        let handle = if entity.common().handle.is_null() {
            let h = self.allocate_handle();
            entity.as_entity_mut().set_handle(h);
            h
        } else {
            let h = entity.common().handle;
            // Ensure the handle counter stays above this handle so
            // future allocations (e.g., vertex sub-entities) don't
            // collide with it.
            if h.value() >= self.next_handle {
                self.next_handle = h.value() + 1;
                self.header.handle_seed = self.next_handle;
            }
            h
        };

        // Set owner to *Model_Space block record if not already set
        let ms_handle = self.header.model_space_block_handle;
        if entity.common().owner_handle.is_null() && !ms_handle.is_null() {
            entity.common_mut().owner_handle = ms_handle;
        }

        // AttributeEntity is a sub-entity owned by INSERT, not a direct
        // block-record child.  Never add it to entity_handles.
        // Block/BlockEnd are structural markers with separate handle fields.
        let is_excluded = matches!(&entity, EntityType::AttributeEntity(_) | EntityType::Block(_) | EntityType::BlockEnd(_));

        // Route entity handle to the correct block record based on owner handle.
        let owner = entity.common().owner_handle;
        let mut added_to_block = false;
        if !is_excluded && !owner.is_null() {
            for br in self.block_records.iter_mut() {
                if br.handle == owner {
                    br.entity_handles.push(handle);
                    added_to_block = true;
                    break;
                }
            }
        }
        // Fallback: add to *Model_Space if owner didn't match any block record
        if !is_excluded && !added_to_block {
            if let Some(ms) = self.block_records.get_mut("*Model_Space") {
                ms.entity_handles.push(handle);
                // Fix the entity's owner so the writer can determine
                // entity_mode correctly (model-space = 2).
                entity.common_mut().owner_handle = ms.handle;
            }
        }

        // Store in the flat entity map (DXF writer reads from here)
        let idx = self.entities.len();
        self.entities.push(entity);
        self.entity_index.insert(handle, idx);
        Ok(handle)
    }

    /// Get an entity by handle
    pub fn get_entity(&self, handle: Handle) -> Option<&EntityType> {
        self.entity_index.get(&handle).map(|&idx| &self.entities[idx])
    }

    /// Get a mutable entity by handle
    pub fn get_entity_mut(&mut self, handle: Handle) -> Option<&mut EntityType> {
        let idx = *self.entity_index.get(&handle)?;
        Some(&mut self.entities[idx])
    }

    /// Explode an entity into simpler primitives, allocating valid handles.
    ///
    /// Each resulting entity receives a unique handle from the document's
    /// handle allocator and inherits the original entity's owner handle.
    /// The caller can then add the returned entities to the document via
    /// [`add_entity`](Self::add_entity) or use them directly.
    ///
    /// Returns an empty `Vec` for atomic entities that cannot be decomposed.
    pub fn explode_entity(&mut self, entity: &EntityType) -> Vec<EntityType> {
        let mut parts = entity.explode();
        let owner = entity.common().owner_handle;
        for part in &mut parts {
            let h = self.allocate_handle();
            part.as_entity_mut().set_handle(h);
            if !owner.is_null() && part.common().owner_handle.is_null() {
                part.common_mut().owner_handle = owner;
            }
        }
        parts
    }

    /// Add an entity to the default paper space (`*Paper_Space` / "Layout1").
    ///
    /// This sets the entity's owner to the `*Paper_Space` block record and
    /// stores it there.  Viewports must be placed in paper space to be
    /// visible in a layout.
    ///
    /// For documents with multiple layouts, use
    /// [`add_entity_to_layout`](Self::add_entity_to_layout) instead.
    pub fn add_paper_space_entity(&mut self, entity: EntityType) -> Result<Handle> {
        self.add_entity_to_block(entity, "*Paper_Space")
    }

    /// Add an entity to a named layout.
    ///
    /// Looks up the [`Layout`](crate::objects::Layout) object by name (e.g.
    /// `"Layout1"`, `"Layout2"`) and adds the entity to the layout's
    /// backing block record.  Returns an error if the layout is not found.
    ///
    /// # Example
    /// ```ignore
    /// use acadrust::entities::{Viewport, EntityType};
    ///
    /// let vp = Viewport::new();
    /// document.add_entity_to_layout(EntityType::Viewport(vp), "Layout1")?;
    /// ```
    pub fn add_entity_to_layout(
        &mut self,
        entity: EntityType,
        layout_name: &str,
    ) -> Result<Handle> {
        // Find the Layout object by name to get its block_record handle
        let block_handle = self
            .objects
            .values()
            .find_map(|obj| match obj {
                ObjectType::Layout(layout) if layout.name == layout_name => {
                    Some(layout.block_record)
                }
                _ => None,
            })
            .ok_or_else(|| {
                crate::error::DxfError::Custom(format!(
                    "Layout '{}' not found",
                    layout_name
                ))
            })?;

        // Find the block record name for this handle
        let block_name = self
            .block_records
            .iter()
            .find(|br| br.handle == block_handle)
            .map(|br| br.name().to_string())
            .ok_or_else(|| {
                crate::error::DxfError::Custom(format!(
                    "Block record for layout '{}' not found",
                    layout_name
                ))
            })?;

        self.add_entity_to_block(entity, &block_name)
    }

    /// Add an entity to a named block record.
    ///
    /// Sets the entity's owner handle and routes it to the specified block
    /// record.  Used internally by [`add_entity`](Self::add_entity),
    /// [`add_paper_space_entity`](Self::add_paper_space_entity), and
    /// [`add_entity_to_layout`](Self::add_entity_to_layout).
    fn add_entity_to_block(
        &mut self,
        mut entity: EntityType,
        block_name: &str,
    ) -> Result<Handle> {
        // Allocate a handle if the entity doesn't have one
        let handle = if entity.common().handle.is_null() {
            let h = self.allocate_handle();
            entity.as_entity_mut().set_handle(h);
            h
        } else {
            let h = entity.common().handle;
            if h.value() >= self.next_handle {
                self.next_handle = h.value() + 1;
                self.header.handle_seed = self.next_handle;
            }
            h
        };

        // Set owner to the target block record
        if let Some(br) = self.block_records.get(block_name) {
            entity.common_mut().owner_handle = br.handle;
        }

        // Route entity handle to the block record
        let owner = entity.common().owner_handle;
        let mut added_to_block = false;
        if !owner.is_null() {
            for br in self.block_records.iter_mut() {
                if br.handle == owner {
                    br.entity_handles.push(handle);
                    added_to_block = true;
                    break;
                }
            }
        }
        if !added_to_block {
            if let Some(target) = self.block_records.get_mut(block_name) {
                target.entity_handles.push(handle);
            }
        }

        // Store in the flat entity map
        let idx = self.entities.len();
        self.entities.push(entity);
        self.entity_index.insert(handle, idx);
        Ok(handle)
    }

    /// Remove an entity by handle
    pub fn remove_entity(&mut self, handle: Handle) -> Option<EntityType> {
        let idx = self.entity_index.remove(&handle)?;
        let entity = self.entities.swap_remove(idx);
        // If the swap moved an element, update its index
        if idx < self.entities.len() {
            let moved_handle = self.entities[idx].common().handle;
            self.entity_index.insert(moved_handle, idx);
        }
        Some(entity)
    }

    /// Add a new paper space layout to the document.
    ///
    /// Creates the backing `*Paper_Space<N>` block record, a [`Layout`]
    /// object, and registers both in the ACAD_LAYOUT dictionary.  Returns
    /// the layout handle.
    ///
    /// # Example
    /// ```ignore
    /// let layout_handle = document.add_layout("Layout2")?;
    /// // Then add entities to it:
    /// document.add_entity_to_layout(EntityType::Viewport(vp), "Layout2")?;
    /// ```
    pub fn add_layout(&mut self, name: &str) -> Result<Handle> {
        // Check for duplicate layout name
        let already_exists = self.objects.values().any(|obj| {
            matches!(obj, ObjectType::Layout(l) if l.name == name)
        });
        if already_exists {
            return Err(crate::error::DxfError::Custom(format!(
                "Layout '{}' already exists",
                name
            )));
        }

        // Determine the next *Paper_Space block name.
        // AutoCAD uses: *Paper_Space, *Paper_Space0, *Paper_Space1, …
        let ps_count = self
            .block_records
            .iter()
            .filter(|br| br.is_paper_space())
            .count();
        let block_name = if ps_count == 0 {
            "*Paper_Space".to_string()
        } else {
            format!("*Paper_Space{}", ps_count - 1)
        };

        // Create the block record
        let mut block_record = BlockRecord::new(&block_name);
        block_record.set_handle(self.allocate_handle());
        block_record.block_entity_handle = self.allocate_handle();
        block_record.block_end_handle = self.allocate_handle();
        let br_handle = block_record.handle;

        // Create the Layout object
        let layout_handle = self.allocate_handle();
        let mut layout = crate::objects::Layout::new(name);
        layout.handle = layout_handle;
        layout.owner = self.header.acad_layout_dict_handle;
        layout.tab_order = ps_count as i16 + 1;
        layout.block_record = br_handle;

        // Link block record → layout
        block_record.layout = layout_handle;
        self.block_records.add(block_record).map_err(|e| {
            crate::error::DxfError::Custom(e)
        })?;

        // Create the overall paper space viewport (ID=1) for this layout.
        // Every paper space layout requires this entity.
        let mut overall_vp = crate::entities::Viewport::new();
        overall_vp.id = 1;
        overall_vp.status = crate::entities::ViewportStatusFlags::default_on();
        let overall_vp_handle = self.allocate_handle();
        overall_vp.common.handle = overall_vp_handle;
        overall_vp.common.owner_handle = br_handle;
        layout.viewport = overall_vp_handle;

        if let Some(br) = self.block_records.get_mut(&block_name) {
            br.entity_handles.push(overall_vp_handle);
        }
        let idx = self.entities.len();
        self.entities.push(EntityType::Viewport(overall_vp));
        self.entity_index.insert(overall_vp_handle, idx);

        // Register in ACAD_LAYOUT dictionary
        if let Some(ObjectType::Dictionary(dict)) =
            self.objects.get_mut(&self.header.acad_layout_dict_handle)
        {
            dict.add_entry(name, layout_handle);
        }

        // Store the Layout object
        self.objects.insert(layout_handle, ObjectType::Layout(layout));

        Ok(layout_handle)
    }

    /// Get the number of entities
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Iterate over all entities
    pub fn entities(&self) -> impl Iterator<Item = &EntityType> {
        self.entities.iter()
    }

    /// Iterate over all entities mutably
    pub fn entities_mut(&mut self) -> impl Iterator<Item = &mut EntityType> {
        self.entities.iter_mut()
    }

    /// Resolve handle references after reading a DXF file.
    ///
    /// This performs a simplified version of ACadSharp's two-phase build:
    ///
    /// 1. Assigns owner handles on model-space entities (owner = model space
    ///    block record handle) when the entity has no owner set.
    /// 2. Assigns owner handles on block-owned entities (owner = the block
    ///    record handle) when the entity has no owner set.
    /// 3. Updates `next_handle` to be above the maximum handle seen in the
    ///    document so that subsequent `allocate_handle()` calls produce unique
    ///    values.
    ///
    /// Call this once after loading (the DXF reader calls it automatically).
    pub fn resolve_references(&mut self) {
        // --- 1. Find the max handle in use across the whole document ---
        let mut max_handle: u64 = self.next_handle;

        // Check entities
        for entity in self.entities.iter() {
            let h = entity.common().handle.value();
            if h >= max_handle {
                max_handle = h + 1;
            }
        }

        // Check objects
        for (handle, _) in &self.objects {
            let h = handle.value();
            if h >= max_handle {
                max_handle = h + 1;
            }
        }

        // Check block record handles
        for br in self.block_records.iter() {
            let h = br.handle.value();
            if h >= max_handle {
                max_handle = h + 1;
            }
            for eh in &br.entity_handles {
                let h = eh.value();
                if h >= max_handle {
                    max_handle = h + 1;
                }
            }
        }

        self.next_handle = max_handle;

        // --- 1b. Resolve table handle collisions ---
        // Collect ALL handles used by entries, entities, and objects so we can
        // detect when a table control handle collides with ANY of them.
        let mut used_handles = std::collections::HashSet::new();
        for e in self.layers.iter()       { if !e.handle().is_null() { used_handles.insert(e.handle().value()); } }
        for e in self.line_types.iter()    { if !e.handle().is_null() { used_handles.insert(e.handle().value()); } }
        for e in self.text_styles.iter()   { if !e.handle().is_null() { used_handles.insert(e.handle().value()); } }
        for e in self.vports.iter()        { if !e.handle().is_null() { used_handles.insert(e.handle().value()); } }
        for e in self.views.iter()         { if !e.handle().is_null() { used_handles.insert(e.handle().value()); } }
        for e in self.ucss.iter()          { if !e.handle().is_null() { used_handles.insert(e.handle().value()); } }
        for e in self.app_ids.iter()       { if !e.handle().is_null() { used_handles.insert(e.handle().value()); } }
        for e in self.dim_styles.iter()    { if !e.handle().is_null() { used_handles.insert(e.handle().value()); } }
        for e in self.block_records.iter() { if !e.handle().is_null() { used_handles.insert(e.handle().value()); } }
        for e in self.entities.iter()      { let h = e.common().handle.value(); if h > 0 { used_handles.insert(h); } }
        for (h, _) in &self.objects        { let v = h.value(); if v > 0 { used_handles.insert(v); } }

        // Reassign any table control handle that collides with a used handle
        if used_handles.contains(&self.vports.handle().value()) {
            let h = Handle::new(self.next_handle); self.next_handle += 1;
            self.vports.set_handle(h); self.header.vport_control_handle = h;
        }
        if used_handles.contains(&self.line_types.handle().value()) {
            let h = Handle::new(self.next_handle); self.next_handle += 1;
            self.line_types.set_handle(h); self.header.linetype_control_handle = h;
        }
        if used_handles.contains(&self.layers.handle().value()) {
            let h = Handle::new(self.next_handle); self.next_handle += 1;
            self.layers.set_handle(h); self.header.layer_control_handle = h;
        }
        if used_handles.contains(&self.text_styles.handle().value()) {
            let h = Handle::new(self.next_handle); self.next_handle += 1;
            self.text_styles.set_handle(h); self.header.style_control_handle = h;
        }
        if used_handles.contains(&self.views.handle().value()) {
            let h = Handle::new(self.next_handle); self.next_handle += 1;
            self.views.set_handle(h); self.header.view_control_handle = h;
        }
        if used_handles.contains(&self.ucss.handle().value()) {
            let h = Handle::new(self.next_handle); self.next_handle += 1;
            self.ucss.set_handle(h); self.header.ucs_control_handle = h;
        }
        if used_handles.contains(&self.app_ids.handle().value()) {
            let h = Handle::new(self.next_handle); self.next_handle += 1;
            self.app_ids.set_handle(h); self.header.appid_control_handle = h;
        }
        if used_handles.contains(&self.dim_styles.handle().value()) {
            let h = Handle::new(self.next_handle); self.next_handle += 1;
            self.dim_styles.set_handle(h); self.header.dimstyle_control_handle = h;
        }
        if used_handles.contains(&self.block_records.handle().value()) {
            let h = Handle::new(self.next_handle); self.next_handle += 1;
            self.block_records.set_handle(h); self.header.block_control_handle = h;
        }

        // --- 1c. Resolve block entity/end handle collisions ---
        // block_entity_handle and block_end_handle are pre-allocated during
        // initialize_defaults() and may collide with entry/entity handles
        // read from the file.
        for br in self.block_records.iter_mut() {
            if !br.block_entity_handle.is_null()
                && used_handles.contains(&br.block_entity_handle.value())
            {
                let h = Handle::new(self.next_handle); self.next_handle += 1;
                br.block_entity_handle = h;
            }
            if !br.block_end_handle.is_null()
                && used_handles.contains(&br.block_end_handle.value())
            {
                let h = Handle::new(self.next_handle); self.next_handle += 1;
                br.block_end_handle = h;
            }
        }

        // --- 1d. Resolve object handle collisions ---
        // Dictionary and other objects created by initialize_defaults() may
        // have handles that collide with file-sourced handles.
        let mut remap: Vec<(Handle, Handle)> = Vec::new();
        let obj_handles: Vec<Handle> = self.objects.keys().copied().collect();
        for old_h in obj_handles {
            if used_handles.contains(&old_h.value()) {
                let new_h = Handle::new(self.next_handle); self.next_handle += 1;
                remap.push((old_h, new_h));
            }
        }
        for (old_h, new_h) in &remap {
            if let Some(mut obj) = self.objects.remove(old_h) {
                // Update the object's own handle field
                match &mut obj {
                    ObjectType::Dictionary(d) => d.handle = *new_h,
                    ObjectType::Layout(l) => l.handle = *new_h,
                    ObjectType::MLineStyle(m) => m.handle = *new_h,
                    ObjectType::PlaceHolder(p) => p.handle = *new_h,
                    ObjectType::DictionaryWithDefault(d) => d.handle = *new_h,
                    _ => {}
                }
                self.objects.insert(*new_h, obj);
            }
        }
        // Update cross-references: dictionary entries and owner handles
        if !remap.is_empty() {
            let remap_map: std::collections::HashMap<u64, Handle> =
                remap.iter().map(|(o, n)| (o.value(), *n)).collect();

            // Update dictionary entry values that reference remapped handles
            for (_, obj) in self.objects.iter_mut() {
                match obj {
                    ObjectType::Dictionary(d) => {
                        if let Some(new_owner) = remap_map.get(&d.owner.value()) {
                            d.owner = *new_owner;
                        }
                        for (_, entry_handle) in d.entries.iter_mut() {
                            if let Some(new_h) = remap_map.get(&entry_handle.value()) {
                                *entry_handle = *new_h;
                            }
                        }
                    }
                    ObjectType::Layout(l) => {
                        if let Some(new_owner) = remap_map.get(&l.owner.value()) {
                            l.owner = *new_owner;
                        }
                    }
                    ObjectType::MLineStyle(m) => {
                        if let Some(new_owner) = remap_map.get(&m.owner.value()) {
                            m.owner = *new_owner;
                        }
                    }
                    ObjectType::PlaceHolder(p) => {
                        if let Some(new_owner) = remap_map.get(&p.owner.value()) {
                            p.owner = *new_owner;
                        }
                    }
                    ObjectType::DictionaryWithDefault(d) => {
                        if let Some(new_owner) = remap_map.get(&d.owner.value()) {
                            d.owner = *new_owner;
                        }
                        for (_, entry_handle) in d.entries.iter_mut() {
                            if let Some(new_h) = remap_map.get(&entry_handle.value()) {
                                *entry_handle = *new_h;
                            }
                        }
                    }
                    _ => {}
                }
            }

            // Update header handles that reference remapped objects
            let header_handles = [
                &mut self.header.named_objects_dict_handle,
                &mut self.header.acad_group_dict_handle,
                &mut self.header.acad_mlinestyle_dict_handle,
                &mut self.header.acad_layout_dict_handle,
                &mut self.header.acad_plotsettings_dict_handle,
                &mut self.header.acad_plotstylename_dict_handle,
                &mut self.header.acad_material_dict_handle,
                &mut self.header.acad_color_dict_handle,
                &mut self.header.acad_visualstyle_dict_handle,
                &mut self.header.current_multiline_style_handle,
            ];
            for handle in header_handles {
                if let Some(new_h) = remap_map.get(&handle.value()) {
                    *handle = *new_h;
                }
            }

            // Update block record layout references
            for br in self.block_records.iter_mut() {
                if let Some(new_h) = remap_map.get(&br.layout.value()) {
                    br.layout = *new_h;
                }
            }
        }
        let model_handle = self.header.model_space_block_handle;
        let paper_handle = self.header.paper_space_block_handle;

        // Model-space entities (document.entities) — use model space as default owner
        for entity in self.entities.iter_mut() {
            let common = match entity {
                EntityType::Dimension(d) => {
                    let base = d.base_mut();
                    &mut base.common
                }
                _ => {
                    // For all other entity types, use as_entity_mut().set_handle pattern
                    // but we need &mut EntityCommon directly — use a helper
                    get_common_mut(entity)
                }
            };
            if common.owner_handle.is_null() {
                common.owner_handle = model_handle;
            }
        }

        // Block record entities — set owner handle on entities looked up from entity map
        for br in self.block_records.iter() {
            let br_handle = br.handle;
            for eh in &br.entity_handles {
                if let Some(&idx) = self.entity_index.get(eh) {
                    let entity = &mut self.entities[idx];
                    let common = match entity {
                        EntityType::Dimension(d) => {
                            let base = d.base_mut();
                            &mut base.common
                        }
                        _ => get_common_mut(entity),
                    };
                    if common.owner_handle.is_null() {
                        common.owner_handle = br_handle;
                    }
                }
            }
        }

        // Paper-space entities — if an entity's owner is the paper space block,
        // the entity is already correctly assigned by the reader.
        // We just skip further assignment here.

        let _ = paper_handle; // suppress unused warning; future: paper space logic
    }
}

/// Helper to get a mutable reference to EntityCommon for non-Dimension entities.
fn get_common_mut(entity: &mut EntityType) -> &mut EntityCommon {
    match entity {
        EntityType::Point(e) => &mut e.common,
        EntityType::Line(e) => &mut e.common,
        EntityType::Circle(e) => &mut e.common,
        EntityType::Arc(e) => &mut e.common,
        EntityType::Ellipse(e) => &mut e.common,
        EntityType::Polyline(e) => &mut e.common,
        EntityType::Polyline2D(e) => &mut e.common,
        EntityType::Polyline3D(e) => &mut e.common,
        EntityType::LwPolyline(e) => &mut e.common,
        EntityType::Text(e) => &mut e.common,
        EntityType::MText(e) => &mut e.common,
        EntityType::Spline(e) => &mut e.common,
        EntityType::Dimension(d) => &mut d.base_mut().common,
        EntityType::Hatch(e) => &mut e.common,
        EntityType::Solid(e) => &mut e.common,
        EntityType::Face3D(e) => &mut e.common,
        EntityType::Insert(e) => &mut e.common,
        EntityType::Block(e) => &mut e.common,
        EntityType::BlockEnd(e) => &mut e.common,
        EntityType::Ray(e) => &mut e.common,
        EntityType::XLine(e) => &mut e.common,
        EntityType::Viewport(e) => &mut e.common,
        EntityType::AttributeDefinition(e) => &mut e.common,
        EntityType::AttributeEntity(e) => &mut e.common,
        EntityType::Leader(e) => &mut e.common,
        EntityType::MultiLeader(e) => &mut e.common,
        EntityType::MLine(e) => &mut e.common,
        EntityType::Mesh(e) => &mut e.common,
        EntityType::RasterImage(e) => &mut e.common,
        EntityType::Solid3D(e) => &mut e.common,
        EntityType::Region(e) => &mut e.common,
        EntityType::Body(e) => &mut e.common,
        EntityType::Table(e) => &mut e.common,
        EntityType::Tolerance(e) => &mut e.common,
        EntityType::PolyfaceMesh(e) => &mut e.common,
        EntityType::Wipeout(e) => &mut e.common,
        EntityType::Shape(e) => &mut e.common,
        EntityType::Underlay(e) => &mut e.common,
        EntityType::Seqend(e) => &mut e.common,
        EntityType::Ole2Frame(e) => &mut e.common,
        EntityType::PolygonMesh(e) => &mut e.common,
        EntityType::Unknown(e) => &mut e.common,
    }
}

impl Default for CadDocument {
    fn default() -> Self {
        Self::new()
    }
}


