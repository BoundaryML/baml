use crate::{
    baml::cffi::{cffi_raw_object::Object, CffiPointerType, CffiRawObject},
    ctypes::utils::{Decode, Encode},
    raw_ptr_wrapper::{
        CollectorWrapper, FunctionLogWrapper, HTTPBodyWrapper, HTTPRequestWrapper,
        HTTPResponseWrapper, LLMCallWrapper, LLMStreamCallWrapper, MediaWrapper, RawPtrType,
        RawPtrWrapper, SSEEventWrapper, StreamTimingWrapper, TimingWrapper, UsageWrapper,
    },
};

impl Decode for RawPtrType {
    type From = CffiRawObject;

    fn decode(from: Self::From) -> Result<Self, anyhow::Error>
    where
        Self: Sized,
    {
        match from.object {
            Some(Object::Collector(pointer)) => {
                Ok(RawPtrType::Collector(CollectorWrapper::decode(pointer)?))
            }
            Some(Object::Usage(pointer)) => Ok(RawPtrType::Usage(UsageWrapper::decode(pointer)?)),
            Some(Object::FunctionLog(pointer)) => Ok(RawPtrType::FunctionLog(
                FunctionLogWrapper::decode(pointer)?,
            )),
            Some(Object::Timing(pointer)) => {
                Ok(RawPtrType::Timing(TimingWrapper::decode(pointer)?))
            }
            Some(Object::StreamTiming(pointer)) => Ok(RawPtrType::StreamTiming(
                StreamTimingWrapper::decode(pointer)?,
            )),
            Some(Object::LlmCall(pointer)) => {
                Ok(RawPtrType::LLMCall(LLMCallWrapper::decode(pointer)?))
            }
            Some(Object::LlmStreamCall(pointer)) => Ok(RawPtrType::LLMStreamCall(
                LLMStreamCallWrapper::decode(pointer)?,
            )),
            Some(Object::HttpRequest(pointer)) => Ok(RawPtrType::HTTPRequest(
                HTTPRequestWrapper::decode(pointer)?,
            )),
            Some(Object::HttpResponse(pointer)) => Ok(RawPtrType::HTTPResponse(
                HTTPResponseWrapper::decode(pointer)?,
            )),
            Some(Object::HttpBody(pointer)) => {
                Ok(RawPtrType::HTTPBody(HTTPBodyWrapper::decode(pointer)?))
            }
            Some(Object::SseResponse(pointer)) => {
                Ok(RawPtrType::SSEEvent(SSEEventWrapper::decode(pointer)?))
            }
            Some(Object::MediaImage(pointer)) => {
                Ok(RawPtrType::Media(MediaWrapper::decode(pointer)?))
            }
            Some(Object::MediaAudio(pointer)) => {
                Ok(RawPtrType::Media(MediaWrapper::decode(pointer)?))
            }
            Some(Object::MediaPdf(pointer)) => {
                Ok(RawPtrType::Media(MediaWrapper::decode(pointer)?))
            }
            Some(Object::MediaVideo(pointer)) => {
                Ok(RawPtrType::Media(MediaWrapper::decode(pointer)?))
            }
            None => Err(anyhow::anyhow!("Invalid object type")),
        }
    }
}

impl Encode<CffiRawObject> for RawPtrType {
    fn encode(self) -> CffiRawObject {
        match self {
            RawPtrType::Collector(raw_ptr_wrapper) => raw_ptr_wrapper.encode(),
            RawPtrType::Usage(raw_ptr_wrapper) => raw_ptr_wrapper.encode(),
            RawPtrType::FunctionLog(raw_ptr_wrapper) => raw_ptr_wrapper.encode(),
            RawPtrType::Timing(raw_ptr_wrapper) => raw_ptr_wrapper.encode(),
            RawPtrType::StreamTiming(raw_ptr_wrapper) => raw_ptr_wrapper.encode(),
            RawPtrType::LLMCall(raw_ptr_wrapper) => raw_ptr_wrapper.encode(),
            RawPtrType::LLMStreamCall(raw_ptr_wrapper) => raw_ptr_wrapper.encode(),
            RawPtrType::HTTPRequest(raw_ptr_wrapper) => raw_ptr_wrapper.encode(),
            RawPtrType::HTTPResponse(raw_ptr_wrapper) => raw_ptr_wrapper.encode(),
            RawPtrType::HTTPBody(raw_ptr_wrapper) => raw_ptr_wrapper.encode(),
            RawPtrType::SSEEvent(raw_ptr_wrapper) => raw_ptr_wrapper.encode(),
            RawPtrType::Media(raw_ptr_wrapper) => raw_ptr_wrapper.encode(),
        }
    }
}

macro_rules! impl_encode_decode_for_wrapper {
    ($object_type:ident, $wrapper_type:ident) => {
        impl Decode for $wrapper_type {
            type From = CffiPointerType;

            fn decode(from: Self::From) -> Result<Self, anyhow::Error>
            where
                Self: Sized,
            {
                Ok($wrapper_type::from_raw(
                    from.pointer as *const libc::c_void,
                    true,
                ))
            }
        }

        impl ObjectType for $wrapper_type {
            fn object_type(&self) -> Object {
                Object::$object_type(self.pointer())
            }
        }
    };
}

trait ObjectType {
    fn object_type(&self) -> Object;
}

impl<T> Encode<CffiRawObject> for RawPtrWrapper<T>
where
    RawPtrWrapper<T>: ObjectType,
{
    fn encode(self) -> CffiRawObject {
        CffiRawObject {
            object: Some(self.object_type()),
        }
    }
}

impl_encode_decode_for_wrapper!(Collector, CollectorWrapper);
impl_encode_decode_for_wrapper!(Usage, UsageWrapper);
impl_encode_decode_for_wrapper!(FunctionLog, FunctionLogWrapper);
impl_encode_decode_for_wrapper!(Timing, TimingWrapper);
impl_encode_decode_for_wrapper!(StreamTiming, StreamTimingWrapper);
impl_encode_decode_for_wrapper!(LlmCall, LLMCallWrapper);
impl_encode_decode_for_wrapper!(LlmStreamCall, LLMStreamCallWrapper);
impl_encode_decode_for_wrapper!(HttpRequest, HTTPRequestWrapper);
impl_encode_decode_for_wrapper!(HttpResponse, HTTPResponseWrapper);
impl_encode_decode_for_wrapper!(HttpBody, HTTPBodyWrapper);
impl_encode_decode_for_wrapper!(SseResponse, SSEEventWrapper);

impl Decode for MediaWrapper {
    type From = CffiPointerType;

    fn decode(from: Self::From) -> Result<Self, anyhow::Error>
    where
        Self: Sized,
    {
        Ok(MediaWrapper::from_raw(
            from.pointer as *const libc::c_void,
            true,
        ))
    }
}

impl ObjectType for MediaWrapper {
    fn object_type(&self) -> Object {
        match self.media_type {
            baml_types::BamlMediaType::Image => Object::MediaImage(self.pointer()),
            baml_types::BamlMediaType::Audio => Object::MediaAudio(self.pointer()),
            baml_types::BamlMediaType::Pdf => Object::MediaPdf(self.pointer()),
            baml_types::BamlMediaType::Video => Object::MediaVideo(self.pointer()),
        }
    }
}
