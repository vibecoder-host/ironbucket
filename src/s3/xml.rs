use crate::storage::{BucketInfo, ListObjectsResult, ObjectMetadata};
use super::handlers::ListObjectsParams;

pub fn list_buckets_response(buckets: &[BucketInfo]) -> String {
    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push_str("\n<ListAllMyBucketsResult>");
    xml.push_str("\n  <Owner>");
    xml.push_str("\n    <ID>rustybucket</ID>");
    xml.push_str("\n    <DisplayName>RustyBucket User</DisplayName>");
    xml.push_str("\n  </Owner>");
    xml.push_str("\n  <Buckets>");

    for bucket in buckets {
        xml.push_str(&format!(
            "\n    <Bucket>\n      <Name>{}</Name>\n      <CreationDate>{}</CreationDate>\n    </Bucket>",
            bucket.name,
            bucket.created.to_rfc3339()
        ));
    }

    xml.push_str("\n  </Buckets>");
    xml.push_str("\n</ListAllMyBucketsResult>");
    xml
}

pub fn list_objects_response(
    bucket: &str,
    result: &ListObjectsResult,
    params: &ListObjectsParams,
) -> String {
    let list_type = params.list_type.unwrap_or(2);

    if list_type == 2 {
        list_objects_v2_response(bucket, result, params)
    } else {
        list_objects_v1_response(bucket, result, params)
    }
}

fn list_objects_v2_response(
    bucket: &str,
    result: &ListObjectsResult,
    params: &ListObjectsParams,
) -> String {
    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push_str("\n<ListBucketResult>");
    xml.push_str(&format!("\n  <Name>{}</Name>", bucket));

    if let Some(prefix) = &params.prefix {
        xml.push_str(&format!("\n  <Prefix>{}</Prefix>", prefix));
    } else {
        xml.push_str("\n  <Prefix></Prefix>");
    }

    if let Some(delimiter) = &params.delimiter {
        xml.push_str(&format!("\n  <Delimiter>{}</Delimiter>", delimiter));
    }

    xml.push_str(&format!("\n  <MaxKeys>{}</MaxKeys>", params.max_keys.unwrap_or(1000)));
    xml.push_str(&format!("\n  <KeyCount>{}</KeyCount>", result.objects.len()));
    xml.push_str(&format!("\n  <IsTruncated>{}</IsTruncated>", result.is_truncated));

    if let Some(token) = &result.next_continuation_token {
        xml.push_str(&format!("\n  <NextContinuationToken>{}</NextContinuationToken>", token));
    }

    // Add objects
    for object in &result.objects {
        xml.push_str("\n  <Contents>");
        xml.push_str(&format!("\n    <Key>{}</Key>", object.key));
        xml.push_str(&format!("\n    <LastModified>{}</LastModified>", object.last_modified.to_rfc3339()));
        xml.push_str(&format!("\n    <ETag>\"{}\"</ETag>", object.etag));
        xml.push_str(&format!("\n    <Size>{}</Size>", object.size));
        xml.push_str(&format!("\n    <StorageClass>{}</StorageClass>", object.storage_class));
        xml.push_str("\n  </Contents>");
    }

    // Add common prefixes
    for prefix in &result.prefixes {
        xml.push_str("\n  <CommonPrefixes>");
        xml.push_str(&format!("\n    <Prefix>{}</Prefix>", prefix));
        xml.push_str("\n  </CommonPrefixes>");
    }

    xml.push_str("\n</ListBucketResult>");
    xml
}

fn list_objects_v1_response(
    bucket: &str,
    result: &ListObjectsResult,
    params: &ListObjectsParams,
) -> String {
    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push_str("\n<ListBucketResult>");
    xml.push_str(&format!("\n  <Name>{}</Name>", bucket));

    if let Some(prefix) = &params.prefix {
        xml.push_str(&format!("\n  <Prefix>{}</Prefix>", prefix));
    } else {
        xml.push_str("\n  <Prefix></Prefix>");
    }

    if let Some(delimiter) = &params.delimiter {
        xml.push_str(&format!("\n  <Delimiter>{}</Delimiter>", delimiter));
    }

    xml.push_str(&format!("\n  <MaxKeys>{}</MaxKeys>", params.max_keys.unwrap_or(1000)));
    xml.push_str(&format!("\n  <IsTruncated>{}</IsTruncated>", result.is_truncated));

    // Add objects
    for object in &result.objects {
        xml.push_str("\n  <Contents>");
        xml.push_str(&format!("\n    <Key>{}</Key>", object.key));
        xml.push_str(&format!("\n    <LastModified>{}</LastModified>", object.last_modified.to_rfc3339()));
        xml.push_str(&format!("\n    <ETag>\"{}\"</ETag>", object.etag));
        xml.push_str(&format!("\n    <Size>{}</Size>", object.size));
        xml.push_str(&format!("\n    <StorageClass>{}</StorageClass>", object.storage_class));
        xml.push_str("\n    <Owner>");
        xml.push_str("\n      <ID>rustybucket</ID>");
        xml.push_str("\n      <DisplayName>RustyBucket User</DisplayName>");
        xml.push_str("\n    </Owner>");
        xml.push_str("\n  </Contents>");
    }

    // Add common prefixes
    for prefix in &result.prefixes {
        xml.push_str("\n  <CommonPrefixes>");
        xml.push_str(&format!("\n    <Prefix>{}</Prefix>", prefix));
        xml.push_str("\n  </CommonPrefixes>");
    }

    xml.push_str("\n</ListBucketResult>");
    xml
}