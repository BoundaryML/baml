use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use baml_rpc::runtime_api::baml_value::{BamlValue, MediaValue, ValueContent};
use base64::{engine::general_purpose, Engine as _};
use blake3;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::tracingv2::publisher::publisher::BlobUploaderMessage;

/// Represents a blob that needs to be uploaded
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobMetadata {
    pub blob_hash: String,
    pub function_call_id: String,
    pub media_type: Option<String>,
    pub size_bytes: usize,
}

/// Represents a blob with its content
#[derive(Debug, Clone)]
pub struct BlobWithContent {
    pub metadata: BlobMetadata,
    pub content: Vec<u8>,
}

/// Cache for managing blob references and uploads
#[derive(Clone)]
pub struct BlobRefCache {
    // Maps blob_hash -> (content, set of function_call_ids using this blob)
    blobs: Arc<Mutex<HashMap<String, (Vec<u8>, HashSet<String>)>>>,
    // Tracks which function_call_ids are active
    active_calls: Arc<Mutex<HashSet<String>>>,
    // Channel to queue blobs for immediate upload (bounded to prevent unbounded memory growth)
    blob_upload_tx: Option<mpsc::Sender<BlobUploaderMessage>>,
}

impl Default for BlobRefCache {
    fn default() -> Self {
        Self::new()
    }
}

impl BlobRefCache {
    pub fn new() -> Self {
        Self {
            blobs: Arc::new(Mutex::new(HashMap::new())),
            active_calls: Arc::new(Mutex::new(HashSet::new())),
            blob_upload_tx: None,
        }
    }

    /// Format a blob hash as a blob reference with the <baml_blob>{hash}</baml_blob> format
    pub fn format_blob_ref(blob_hash: &str) -> String {
        format!("<baml_blob>{blob_hash}</baml_blob>")
    }

    /// Extract the hash from a blob reference (removes the <baml_blob></baml_blob> tags)
    pub fn extract_hash_from_ref(blob_ref: &str) -> Option<&str> {
        if blob_ref.starts_with("<baml_blob>") && blob_ref.ends_with("</baml_blob>") {
            let start = "<baml_blob>".len();
            let end = blob_ref.len() - "</baml_blob>".len();
            if start < end {
                Some(&blob_ref[start..end])
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn with_upload_channel(blob_upload_tx: mpsc::Sender<BlobUploaderMessage>) -> Self {
        Self {
            blobs: Arc::new(Mutex::new(HashMap::new())),
            active_calls: Arc::new(Mutex::new(HashSet::new())),
            blob_upload_tx: Some(blob_upload_tx),
        }
    }

    /// Generate a hash for a blob using BLAKE3
    pub fn hash_blob(content: &[u8]) -> String {
        let hash = blake3::hash(content);
        hash.to_hex().to_string()
    }

    /// Store a blob (as base64 string) and associate it with a function_call_id
    /// Returns the blob reference to use (with <baml_blob>{hash}</baml_blob> format)
    pub fn store_blob(
        &self,
        function_call_id: &str,
        base64_content: &str,
        media_type: Option<String>,
    ) -> String {
        log::info!("Storing blob: {function_call_id}");
        let blob_hash = Self::hash_blob(base64_content.as_bytes());

        let mut blobs = self.blobs.lock().unwrap();
        let is_new_blob = !blobs.contains_key(&blob_hash);

        let entry = blobs
            .entry(blob_hash.clone())
            .or_insert_with(|| (base64_content.as_bytes().to_vec(), HashSet::new()));
        entry.1.insert(function_call_id.to_string());

        // If this is a new blob and we have an upload channel, queue it immediately
        if is_new_blob {
            if let Some(ref upload_tx) = self.blob_upload_tx {
                let blob_with_content = BlobWithContent {
                    metadata: BlobMetadata {
                        blob_hash: blob_hash.clone(),
                        function_call_id: function_call_id.to_string(),
                        media_type: media_type.clone(),
                        size_bytes: base64_content.len(),
                    },
                    content: base64_content.as_bytes().to_vec(),
                };

                // Try to queue the blob; if the channel is full, log a warning
                match upload_tx.try_send(BlobUploaderMessage::QueueBlob(blob_with_content)) {
                    Ok(_) => {
                        log::info!("Queued blob {blob_hash} for upload");
                    }
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        log::warn!("Blob upload queue is full (max 4 batches). Dropping blob {blob_hash}. Consider increasing BAML_BLOB_BATCH_SIZE.");
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        log::warn!(
                            "Blob uploader channel is closed. Cannot queue blob {blob_hash}."
                        );
                    }
                }
            }
        }

        let mut active_calls = self.active_calls.lock().unwrap();
        active_calls.insert(function_call_id.to_string());

        Self::format_blob_ref(&blob_hash)
    }

    /// Mark a function call as started
    pub fn start_function_call(&self, function_call_id: &str) {
        // log::info!("Starting function call: {}", function_call_id);
        let mut active_calls = self.active_calls.lock().unwrap();
        active_calls.insert(function_call_id.to_string());
    }

    /// Mark a function call as completed and clean up unused blobs
    pub fn end_function_call(&self, function_call_id: &str) {
        // log::info!("Ending function call: {}", function_call_id);
        let mut active_calls = self.active_calls.lock().unwrap();
        active_calls.remove(function_call_id);

        // Simply remove references - blobs have already been queued for upload
        let mut blobs = self.blobs.lock().unwrap();
        let mut to_remove = Vec::new();

        for (hash, (_, refs)) in blobs.iter_mut() {
            refs.remove(function_call_id);
            // Remove blobs that have no active references
            // Since they're already queued for upload, we don't need to keep them in cache
            if refs.is_empty() {
                to_remove.push(hash.clone());
            }
        }

        // Actually perform the removals
        for hash in to_remove {
            // log::info!("Removing blob {hash} from cache (no active references)");
            blobs.remove(&hash);
        }
    }

    /// Get blobs for a specific function call ID as (base64_content, blob_ref) pairs
    pub fn get_blobs_for_function(&self, function_call_id: &str) -> Vec<(String, String)> {
        let blobs = self.blobs.lock().unwrap();
        blobs
            .iter()
            .filter_map(|(hash, (content, refs))| {
                if refs.contains(function_call_id) {
                    Some((
                        String::from_utf8_lossy(content).to_string(),
                        Self::format_blob_ref(hash),
                    ))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get number of blobs stored for testing
    #[cfg(test)]
    pub fn blob_count(&self) -> usize {
        self.blobs.lock().unwrap().len()
    }

    /// Check if a blob exists for testing (accepts blob reference with <baml_blob></baml_blob> tags)
    #[cfg(test)]
    pub fn has_blob(&self, blob_ref: &str) -> bool {
        if let Some(hash) = Self::extract_hash_from_ref(blob_ref) {
            self.blobs.lock().unwrap().contains_key(hash)
        } else {
            false
        }
    }

    /// Get blob content for testing (accepts blob reference with <baml_blob></baml_blob> tags)
    #[cfg(test)]
    pub fn get_blob_content(&self, blob_ref: &str) -> Option<Vec<u8>> {
        if let Some(hash) = Self::extract_hash_from_ref(blob_ref) {
            self.blobs
                .lock()
                .unwrap()
                .get(hash)
                .map(|(content, _)| content.clone())
        } else {
            None
        }
    }

    /// Get blob reference count for testing (accepts blob reference with <baml_blob></baml_blob> tags)
    #[cfg(test)]
    pub fn get_blob_ref_count(&self, blob_ref: &str) -> usize {
        if let Some(hash) = Self::extract_hash_from_ref(blob_ref) {
            self.blobs
                .lock()
                .unwrap()
                .get(hash)
                .map(|(_, refs)| refs.len())
                .unwrap_or(0)
        } else {
            0
        }
    }

    /// Check if function call is active for testing
    #[cfg(test)]
    pub fn is_function_active(&self, function_call_id: &str) -> bool {
        self.active_calls.lock().unwrap().contains(function_call_id)
    }

    /// Check if blob has specific reference for testing (accepts blob reference with <baml_blob></baml_blob> tags)
    #[cfg(test)]
    pub fn blob_has_ref(&self, blob_ref: &str, function_call_id: &str) -> bool {
        if let Some(hash) = Self::extract_hash_from_ref(blob_ref) {
            self.blobs
                .lock()
                .unwrap()
                .get(hash)
                .map(|(_, refs)| refs.contains(function_call_id))
                .unwrap_or(false)
        } else {
            false
        }
    }
}

/// Trait for blob storage functionality
pub trait BlobStorage {
    fn blob_cache(&self) -> &BlobRefCache;
}

/// Helper for extracting blobs from BamlValue
/// This does a simple replacement of Base64 content with blob references
pub fn extract_blobs_from_baml_value<'a>(
    value: &mut BamlValue<'a>,
    cache: &BlobRefCache,
    function_call_id: &str,
) {
    match &mut value.value {
        ValueContent::Media(media) => {
            if let MediaValue::Base64(base64_str) = &media.value {
                let blob_hash = cache.store_blob(
                    function_call_id,
                    base64_str.as_ref(),
                    media.mime_type.clone(), // Use mime_type from Media struct
                );
                // Replace the Base64 variant with BlobRef containing the hash
                // The original base64 string is now stored in the blob cache
                media.value = MediaValue::BlobRef(Cow::Owned(blob_hash));
            }
        }
        ValueContent::List(items) => {
            for item in items {
                extract_blobs_from_baml_value(item, cache, function_call_id);
            }
        }
        ValueContent::Map(map) => {
            for (_, val) in map {
                extract_blobs_from_baml_value(val, cache, function_call_id);
            }
        }
        ValueContent::Class { fields } => {
            for (_, val) in fields {
                extract_blobs_from_baml_value(val, cache, function_call_id);
            }
        }
        _ => {}
    }
}

/// Helper for extracting blobs from string content (for LLMRequest and RawRequest)
/// This does simple string replacement of base64 content with blob references (format: <baml_blob>{hash}</baml_blob>)
pub fn extract_blobs_from_string(
    content: &str,
    cache: &BlobRefCache,
    function_call_id: &str,
) -> String {
    let mut result = content.to_string();

    // Get blobs for this specific function call ID
    let function_blobs = cache.get_blobs_for_function(function_call_id);

    // For each blob associated with this function call, replace base64 with blob reference
    for (base64_content, blob_ref) in function_blobs {
        if result.contains(&base64_content) {
            result = result.replace(&base64_content, &blob_ref);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blob_cache_storage_and_retrieval() {
        let cache = BlobRefCache::new();
        let function_call_id = "call-123";

        // Store a blob
        let base64_content = "dGVzdCBpbWFnZSBkYXRh"; // "test image data" in base64
        let hash = cache.store_blob(
            function_call_id,
            base64_content,
            Some("image/png".to_string()),
        );

        // Verify blob is stored in internal cache
        assert_eq!(cache.blob_count(), 1);
        assert!(cache.has_blob(&hash));
        assert_eq!(
            cache.get_blob_content(&hash).unwrap(),
            base64_content.as_bytes()
        );
        assert!(cache.blob_has_ref(&hash, function_call_id));

        // Verify we can retrieve blobs for this function
        let function_blobs = cache.get_blobs_for_function(function_call_id);
        assert_eq!(function_blobs.len(), 1);
        assert_eq!(function_blobs[0].0, base64_content);
        assert_eq!(function_blobs[0].1, hash);
    }

    #[test]
    fn test_blob_sharing() {
        let cache = BlobRefCache::new();
        let base64_content = "c2hhcmVkIGltYWdl"; // "shared image" in base64

        // Two function calls use the same blob
        let hash1 = cache.store_blob("call-1", base64_content, None);
        let hash2 = cache.store_blob("call-2", base64_content, None);

        // Should generate the same hash
        assert_eq!(hash1, hash2);

        // Should only have one blob in cache with two references
        assert_eq!(cache.blob_count(), 1);
        assert_eq!(cache.get_blob_ref_count(&hash1), 2);
        assert!(cache.blob_has_ref(&hash1, "call-1"));
        assert!(cache.blob_has_ref(&hash1, "call-2"));

        // Ending one call shouldn't remove the blob
        cache.end_function_call("call-1");
        assert!(cache.has_blob(&hash1));
        assert_eq!(cache.get_blob_ref_count(&hash1), 1);
        assert!(cache.blob_has_ref(&hash1, "call-2"));

        // Ending both calls should remove the blob
        cache.end_function_call("call-2");
        assert_eq!(cache.blob_count(), 0);
    }

    #[test]
    fn test_blob_removal_when_no_references() {
        let cache = BlobRefCache::new();
        let base64_content = "dGVzdCBibG9i"; // "test blob" in base64

        // Scenario: Two functions reference the same blob
        let hash = cache.store_blob("func-a", base64_content, None);
        cache.store_blob("func-b", base64_content, None);

        // Should have one blob with two references
        assert_eq!(cache.blob_count(), 1);
        assert_eq!(cache.get_blob_ref_count(&hash), 2);

        // End first function - blob should remain (still referenced by func-b)
        cache.end_function_call("func-a");
        assert!(cache.has_blob(&hash));
        assert_eq!(cache.get_blob_ref_count(&hash), 1);

        // End second function - blob should be removed (no more references)
        cache.end_function_call("func-b");
        assert!(!cache.has_blob(&hash));
    }

    #[test]
    fn test_concurrent_function_calls() {
        let cache = BlobRefCache::new();
        let base64_content = "Y29uY3VycmVudA=="; // "concurrent" in base64

        // Start multiple function calls
        cache.start_function_call("func-1");
        cache.start_function_call("func-2");
        cache.start_function_call("func-3");

        // All store the same blob
        let hash = cache.store_blob("func-1", base64_content, None);
        cache.store_blob("func-2", base64_content, None);
        cache.store_blob("func-3", base64_content, None);

        // Should have one blob with 3 references
        assert_eq!(cache.blob_count(), 1);
        assert_eq!(cache.get_blob_ref_count(&hash), 3);

        // End functions one by one
        cache.end_function_call("func-1");
        assert!(cache.has_blob(&hash)); // Still has 2 references
        assert_eq!(cache.get_blob_ref_count(&hash), 2);

        cache.end_function_call("func-2");
        assert!(cache.has_blob(&hash)); // Still has 1 reference
        assert_eq!(cache.get_blob_ref_count(&hash), 1);

        cache.end_function_call("func-3");
        assert!(!cache.has_blob(&hash)); // No references, should be removed
    }

    #[test]
    fn test_extract_base64_from_string() {
        let cache = BlobRefCache::new();
        let function_call_id = "call-123";

        // Store a blob first
        let base64_content = "aGVsbG8gd29ybGQ="; // "hello world" in base64
        let blob_hash = cache.store_blob(function_call_id, base64_content, None);

        // Test string that contains the base64 of our stored blob
        let input = format!("Here's an image: {base64_content} and some text");
        let result = extract_blobs_from_string(&input, &cache, function_call_id);

        // Should replace base64 with blob hash
        assert!(result.contains(&blob_hash));
        assert!(!result.contains(base64_content));
        assert!(result.contains("Here's an image:"));
        assert!(result.contains("and some text"));

        // Should have the blob stored in cache
        assert_eq!(cache.blob_count(), 1);
        assert!(cache.has_blob(&blob_hash));
        assert_eq!(
            cache.get_blob_content(&blob_hash).unwrap(),
            base64_content.as_bytes()
        );
    }

    #[test]
    fn test_hash_generation_consistency() {
        let cache = BlobRefCache::new();

        // Same content should generate same hash
        let content = "same content";
        let hash1 = BlobRefCache::hash_blob(content.as_bytes());
        let hash2 = BlobRefCache::hash_blob(content.as_bytes());
        assert_eq!(hash1, hash2);

        // Different content should generate different hashes
        let hash3 = BlobRefCache::hash_blob("different content".as_bytes());
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_function_call_lifecycle() {
        let cache = BlobRefCache::new();

        // Start a function call
        cache.start_function_call("func-1");
        assert!(cache.is_function_active("func-1"));

        // Store a blob for this function
        let hash = cache.store_blob("func-1", "test content", None);

        // End the function call
        cache.end_function_call("func-1");
        assert!(!cache.is_function_active("func-1"));

        // Blob should be removed since no references remain
        assert!(!cache.has_blob(&hash));
    }
}
