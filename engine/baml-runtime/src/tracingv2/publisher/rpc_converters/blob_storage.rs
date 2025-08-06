use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};
use baml_rpc::runtime_api::baml_value::{BamlValue, MediaValue, ValueContent};
use base64::{Engine as _, engine::general_purpose};

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
    // Tracks blobs that have been uploaded but still have active references
    uploaded_blobs: Arc<Mutex<HashSet<String>>>,
}

impl BlobRefCache {
    pub fn new() -> Self {
        Self {
            blobs: Arc::new(Mutex::new(HashMap::new())),
            active_calls: Arc::new(Mutex::new(HashSet::new())),
            uploaded_blobs: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Generate a hash for a blob
    pub fn hash_blob(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        format!("{:x}", hasher.finalize())
    }

    /// Store a blob (as base64 string) and associate it with a function_call_id
    /// Returns the blob hash to use as a reference
    pub fn store_blob(
        &self,
        function_call_id: &str,
        base64_content: &str,
        media_type: Option<String>,
    ) -> String {
        let blob_hash = Self::hash_blob(base64_content.as_bytes());
        
        let mut blobs = self.blobs.lock().unwrap();
        let entry = blobs.entry(blob_hash.clone()).or_insert_with(|| {
            (base64_content.as_bytes().to_vec(), HashSet::new())
        });
        entry.1.insert(function_call_id.to_string());
        
        let mut active_calls = self.active_calls.lock().unwrap();
        active_calls.insert(function_call_id.to_string());
        
        blob_hash
    }

    /// Mark a function call as started
    pub fn start_function_call(&self, function_call_id: &str) {
        let mut active_calls = self.active_calls.lock().unwrap();
        active_calls.insert(function_call_id.to_string());
    }

    /// Mark a function call as completed and clean up unused blobs
    pub fn end_function_call(&self, function_call_id: &str) {
        let mut active_calls = self.active_calls.lock().unwrap();
        active_calls.remove(function_call_id);
        
        // Clean up blobs that are no longer referenced
        let mut blobs = self.blobs.lock().unwrap();
        let uploaded = self.uploaded_blobs.lock().unwrap();
        let mut to_remove = Vec::new();
        
        for (hash, (_, refs)) in blobs.iter_mut() {
            refs.remove(function_call_id);
            // Only remove blobs that have no references and have been uploaded
            if refs.is_empty() && uploaded.contains(hash) {
                to_remove.push(hash.clone());
            }
        }
        
        // Actually perform the removals
        drop(uploaded);
        let mut uploaded = self.uploaded_blobs.lock().unwrap();
        for hash in to_remove {
            blobs.remove(&hash);
            uploaded.remove(&hash);
        }
    }

    /// Get all blobs that need to be uploaded (excludes already uploaded blobs)
    pub fn get_pending_blobs(&self) -> Vec<BlobWithContent> {
        let blobs = self.blobs.lock().unwrap();
        let uploaded = self.uploaded_blobs.lock().unwrap();
        
        blobs
            .iter()
            .filter(|(hash, _)| !uploaded.contains(*hash))
            .map(|(hash, (content, refs))| {
                let function_call_id = refs.iter().next().unwrap().clone();
                BlobWithContent {
                    metadata: BlobMetadata {
                        blob_hash: hash.clone(),
                        function_call_id,
                        media_type: None, // TODO: track media type properly
                        size_bytes: content.len(),
                    },
                    content: content.clone(),
                }
            })
            .collect()
    }

    /// Get blobs for a specific function call ID as (base64_content, blob_hash) pairs
    pub fn get_blobs_for_function(&self, function_call_id: &str) -> Vec<(String, String)> {
        let blobs = self.blobs.lock().unwrap();
        blobs
            .iter()
            .filter_map(|(hash, (content, refs))| {
                if refs.contains(function_call_id) {
                    Some((String::from_utf8_lossy(content).to_string(), hash.clone()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Mark blobs as uploaded. Only removes them from cache if they have no active references.
    pub fn mark_blobs_uploaded(&self, blob_hashes: &[String]) {
        let mut blobs = self.blobs.lock().unwrap();
        let mut uploaded = self.uploaded_blobs.lock().unwrap();
        
        for hash in blob_hashes {
            // Check if this blob still has active references
            if let Some((_, refs)) = blobs.get(hash) {
                if refs.is_empty() {
                    // No active references, safe to remove
                    blobs.remove(hash);
                    uploaded.remove(hash);
                } else {
                    // Still has active references, mark as uploaded but keep in cache
                    uploaded.insert(hash.clone());
                }
            }
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
/// This does simple string replacement of base64 content with blob hashes
pub fn extract_blobs_from_string(
    content: &str,
    cache: &BlobRefCache,
    function_call_id: &str,
) -> String {
    let mut result = content.to_string();
    
    // Get blobs for this specific function call ID
    let function_blobs = cache.get_blobs_for_function(function_call_id);
    
    // For each blob associated with this function call, replace base64 with blob hash
    for (base64_content, blob_hash) in function_blobs {
        if result.contains(&base64_content) {
            result = result.replace(&base64_content, &blob_hash);
        }
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blob_cache_lifecycle() {
        let cache = BlobRefCache::new();
        let function_call_id = "call-123";
        
        // Store a blob
        let base64_content = "dGVzdCBpbWFnZSBkYXRh"; // "test image data" in base64
        let hash = cache.store_blob(function_call_id, base64_content, Some("image/png".to_string()));
        
        // Verify blob is stored
        let pending = cache.get_pending_blobs();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].metadata.blob_hash, hash);
        assert_eq!(pending[0].content, base64_content.as_bytes());
        
        // Mark as uploaded but function still active - blob should not be removed
        cache.mark_blobs_uploaded(&[hash.clone()]);
        
        // Should not appear in pending blobs (it's uploaded)
        let pending = cache.get_pending_blobs();
        assert_eq!(pending.len(), 0);
        
        // End function call should now remove the blob since it's uploaded
        cache.end_function_call(function_call_id);
        
        // Verify blob is completely removed
        let blobs = cache.blobs.lock().unwrap();
        assert_eq!(blobs.len(), 0);
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
        
        // Should only have one blob in cache
        let pending = cache.get_pending_blobs();
        assert_eq!(pending.len(), 1);
        
        // Ending one call shouldn't remove the blob
        cache.end_function_call("call-1");
        let pending = cache.get_pending_blobs();
        assert_eq!(pending.len(), 1);
        
        // Ending both calls should not remove the blob (not uploaded yet)
        cache.end_function_call("call-2");
        let blobs = cache.blobs.lock().unwrap();
        assert_eq!(blobs.len(), 1); // Still in cache, just no references
    }

    #[test]
    fn test_blob_eviction_with_upload() {
        let cache = BlobRefCache::new();
        let base64_content = "dGVzdCBibG9i"; // "test blob" in base64
        
        // Scenario: Two functions reference the same blob
        let hash = cache.store_blob("func-a", base64_content, None);
        cache.store_blob("func-b", base64_content, None);
        
        // Verify blob is pending
        assert_eq!(cache.get_pending_blobs().len(), 1);
        
        // Upload the blob
        cache.mark_blobs_uploaded(&[hash.clone()]);
        
        // Should no longer be in pending (it's uploaded)
        assert_eq!(cache.get_pending_blobs().len(), 0);
        
        // But should still be in cache (has active references)
        let blobs = cache.blobs.lock().unwrap();
        assert!(blobs.contains_key(&hash));
        drop(blobs);
        
        // End first function - blob should remain (still referenced by func-b)
        cache.end_function_call("func-a");
        let blobs = cache.blobs.lock().unwrap();
        assert!(blobs.contains_key(&hash));
        drop(blobs);
        
        // End second function - blob should be removed (uploaded and no references)
        cache.end_function_call("func-b");
        let blobs = cache.blobs.lock().unwrap();
        assert!(!blobs.contains_key(&hash));
    }

    #[test]
    fn test_blob_not_removed_if_not_uploaded() {
        let cache = BlobRefCache::new();
        let base64_content = "bm90IHVwbG9hZGVk"; // "not uploaded" in base64
        
        // Store blob and start function
        let hash = cache.store_blob("func-1", base64_content, None);
        
        // End function without uploading
        cache.end_function_call("func-1");
        
        // Blob should still be in cache (not uploaded)
        let blobs = cache.blobs.lock().unwrap();
        assert!(blobs.contains_key(&hash));
        assert_eq!(blobs.get(&hash).unwrap().1.len(), 0); // No references
    }

    #[test]
    fn test_multiple_blob_lifecycle() {
        let cache = BlobRefCache::new();
        
        // Function A with blob 1
        let blob1 = "YmxvYjE="; // "blob1" in base64
        let hash1 = cache.store_blob("func-a", blob1, None);
        
        // Function B with blob 2 and blob 1
        let blob2 = "YmxvYjI="; // "blob2" in base64
        let hash2 = cache.store_blob("func-b", blob2, None);
        cache.store_blob("func-b", blob1, None); // Reuse blob1
        
        // Should have 2 pending blobs
        assert_eq!(cache.get_pending_blobs().len(), 2);
        
        // Upload blob1
        cache.mark_blobs_uploaded(&[hash1.clone()]);
        
        // Only blob2 should be pending
        let pending = cache.get_pending_blobs();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].metadata.blob_hash, hash2);
        
        // End func-a - blob1 should remain (still referenced by func-b)
        cache.end_function_call("func-a");
        let blobs = cache.blobs.lock().unwrap();
        assert!(blobs.contains_key(&hash1));
        drop(blobs);
        
        // Upload blob2
        cache.mark_blobs_uploaded(&[hash2.clone()]);
        
        // No pending blobs
        assert_eq!(cache.get_pending_blobs().len(), 0);
        
        // End func-b - both blobs should be removed
        cache.end_function_call("func-b");
        let blobs = cache.blobs.lock().unwrap();
        assert!(!blobs.contains_key(&hash1));
        assert!(!blobs.contains_key(&hash2));
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
        let blobs = cache.blobs.lock().unwrap();
        assert_eq!(blobs.len(), 1);
        assert_eq!(blobs.get(&hash).unwrap().1.len(), 3);
        drop(blobs);
        
        // Upload the blob
        cache.mark_blobs_uploaded(&[hash.clone()]);
        
        // End functions one by one
        cache.end_function_call("func-1");
        let blobs = cache.blobs.lock().unwrap();
        assert!(blobs.contains_key(&hash)); // Still has 2 references
        drop(blobs);
        
        cache.end_function_call("func-2");
        let blobs = cache.blobs.lock().unwrap();
        assert!(blobs.contains_key(&hash)); // Still has 1 reference
        drop(blobs);
        
        cache.end_function_call("func-3");
        let blobs = cache.blobs.lock().unwrap();
        assert!(!blobs.contains_key(&hash)); // No references, should be removed
    }

    #[test]
    fn test_upload_marks_correctly() {
        let cache = BlobRefCache::new();
        
        // Create multiple blobs
        let blob1 = "Zmlyc3Q="; // "first" in base64
        let blob2 = "c2Vjb25k"; // "second" in base64
        let blob3 = "dGhpcmQ="; // "third" in base64
        
        let hash1 = cache.store_blob("func-1", blob1, None);
        let hash2 = cache.store_blob("func-2", blob2, None);
        let hash3 = cache.store_blob("func-3", blob3, None);
        
        // All should be pending
        assert_eq!(cache.get_pending_blobs().len(), 3);
        
        // Upload only blob1 and blob3
        cache.mark_blobs_uploaded(&[hash1.clone(), hash3.clone()]);
        
        // Only blob2 should be pending
        let pending = cache.get_pending_blobs();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].metadata.blob_hash, hash2);
        
        // Uploaded blobs should be tracked
        let uploaded = cache.uploaded_blobs.lock().unwrap();
        assert!(uploaded.contains(&hash1));
        assert!(!uploaded.contains(&hash2));
        assert!(uploaded.contains(&hash3));
    }

    #[test]
    fn test_extract_base64_from_string() {
        let cache = BlobRefCache::new();
        let function_call_id = "call-123";
        
        // Store a blob first
        let base64_content = "aGVsbG8gd29ybGQ="; // "hello world" in base64
        let blob_hash = cache.store_blob(function_call_id, base64_content, None);
        
        // Test string that contains the base64 of our stored blob
        let input = format!("Here's an image: {} and some text", base64_content);
        let result = extract_blobs_from_string(&input, &cache, function_call_id);
        
        // Should replace base64 with blob hash
        assert!(result.contains(&blob_hash));
        assert!(!result.contains(base64_content));
        assert!(result.contains("Here's an image:"));
        assert!(result.contains("and some text"));
        
        // Should have stored the blob
        let pending = cache.get_pending_blobs();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].content, base64_content.as_bytes());
    }
}