//! The neutral model the Voyager scene reader returns: plain types that know nothing of any app.
//!
//! Coordinates are metres, Z up (Voyager scenes are Y up; the reader turns them as glTF is turned:
//! x, y, z to x, -z, y).

/// A Voyager scene, read.
#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub models: Vec<Model>,
    pub annotations: Vec<Annotation>,
    /// What the document says of itself and of its object (title, collection record, copyright).
    pub info: Vec<(String, String)>,
}

/// One glTF model of the scene.
#[derive(Debug, Clone)]
pub struct Model {
    /// The name of the scene node that holds the model.
    pub name: String,
    /// The URI of the glTF file chosen, as the document names it.
    pub uri: String,
    /// Its quality in the document (Thumb, Low, Medium, High, Highest).
    pub quality: String,
    /// The glTF file's bytes, as read.
    pub glb: Vec<u8>,
    /// A column-major 4x4 matrix from the glTF's Z-up frame (after the standard turn of Y up to
    /// Z up) into the scene, in metres.
    pub placement: [f64; 16],
}

/// An annotation: a point on a model with a title and text.
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub id: String,
    pub title: String,
    pub lead: String,
    /// The index into `Scene::models` of the model it is on.
    pub model: usize,
    /// Where it points, in the scene (metres, Z up).
    pub position: [f64; 3],
    /// The direction its marker stands out in (unit vector, Z up).
    pub direction: [f64; 3],
}
