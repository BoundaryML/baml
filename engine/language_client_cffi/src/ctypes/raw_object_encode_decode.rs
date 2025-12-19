use baml_cffi_macros::generate_encode_decode_impls;

use crate::{
    baml::cffi::{baml_object_handle::Object, BamlObjectHandle, BamlPointerType},
    ctypes::utils::{Decode, Encode},
    raw_ptr_wrapper::{
        ClassBuilderWrapper, ClassPropertyBuilderWrapper, CollectorWrapper, EnumBuilderWrapper,
        EnumValueBuilderWrapper, FunctionLogWrapper, HTTPBodyWrapper, HTTPRequestWrapper,
        HTTPResponseWrapper, LLMCallWrapper, LLMStreamCallWrapper, MediaWrapper, RawPtrType,
        RawPtrWrapper, SSEEventWrapper, StreamTimingWrapper, TimingWrapper, TypeBuilderWrapper,
        TypeWrapper, UsageWrapper,
    },
};

generate_encode_decode_impls! {
    Collector => Collector as CollectorWrapper: "Collector" (Object::Collector),
    Usage => Usage as UsageWrapper: "Usage" (Object::Usage),
    FunctionLog => FunctionLog as FunctionLogWrapper: "FunctionLog" (Object::FunctionLog),
    Timing => Timing as TimingWrapper: "Timing" (Object::Timing),
    StreamTiming => StreamTiming as StreamTimingWrapper: "StreamTiming" (Object::StreamTiming),
    LLMCall => LLMCall as LLMCallWrapper: "LLMCall" (Object::LlmCall),
    LLMStreamCall => LLMStreamCall as LLMStreamCallWrapper: "LLMStreamCall" (Object::LlmStreamCall),
    HTTPRequest => HTTPRequest as HTTPRequestWrapper: "HTTPRequest" (Object::HttpRequest),
    HTTPResponse => HTTPResponse as HTTPResponseWrapper: "HTTPResponse" (Object::HttpResponse),
    HTTPBody => HTTPBody as HTTPBodyWrapper: "HTTPBody" (Object::HttpBody),
    SSEEvent => SSEEvent as SSEEventWrapper: "SSEEvent" (Object::SseResponse),
    BamlMedia => Media as MediaWrapper: "Media" (Object::MediaImage, Object::MediaAudio, Object::MediaPdf, Object::MediaVideo),
    TypeBuilder => TypeBuilder as TypeBuilderWrapper: "TypeBuilder" (Object::TypeBuilder),
    EnumBuilder => EnumBuilder as EnumBuilderWrapper: "EnumBuilder" (Object::EnumBuilder),
    EnumValueBuilder => EnumValueBuilder as EnumValueBuilderWrapper: "EnumValueBuilder" (Object::EnumValueBuilder),
    ClassBuilder => ClassBuilder as ClassBuilderWrapper: "ClassBuilder" (Object::ClassBuilder),
    ClassPropertyBuilder => ClassPropertyBuilder as ClassPropertyBuilderWrapper: "ClassPropertyBuilder" (Object::ClassPropertyBuilder),
    TypeIR => TypeDef as TypeWrapper: "Type" (Object::Type)
}

trait ObjectType {
    fn object_type(&self) -> Object;
}

impl<T> Encode<BamlObjectHandle> for RawPtrWrapper<T>
where
    RawPtrWrapper<T>: ObjectType,
{
    fn encode(self) -> BamlObjectHandle {
        BamlObjectHandle {
            object: Some(self.object_type()),
        }
    }
}

impl Decode for MediaWrapper {
    type From = BamlPointerType;

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
