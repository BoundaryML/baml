// Add alongside the shared interfaces fixture.
class ProjectionRecord {
    text: string,
}

class RecordDecoder {
    implements Decoder {
        type Output = ProjectionRecord
        type Error = never
        function decode(self, input: image) -> ProjectionRecord throws never {
            ProjectionRecord { text: input.base64() }
        }
    }
}

function record_decoder() -> Decoder<Output = ProjectionRecord, Error = never> throws never {
    RecordDecoder {  }
}
