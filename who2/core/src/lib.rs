pub mod core {
    mod model;
    mod chat;
    mod embedding;
    
    pub use model::Model;
    pub use chat::Chat;
    pub use embedding::Embedding;
}