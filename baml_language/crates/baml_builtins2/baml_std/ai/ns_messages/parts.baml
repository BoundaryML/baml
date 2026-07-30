class TextPart {
    value: string,
    metadata: map<string, json>,

    implements ai.MessagePart {
        function kind(self) -> ai.MessagePartKind throws never {
            ai.MessagePartKind.Text
        }
        function text(self) -> string? throws never {
            self.value
        }
        function annotations(self) -> map<string, json> throws never {
            self.metadata
        }
    }
}

class ImagePart {
    value: image,
    metadata: map<string, json>,

    implements ai.MessagePart {
        function kind(self) -> ai.MessagePartKind throws never {
            ai.MessagePartKind.Image
        }
        function media(self) -> image | audio | video | pdf | null throws never {
            self.value
        }
        function annotations(self) -> map<string, json> throws never {
            self.metadata
        }
    }
}

class AudioPart {
    value: audio,
    metadata: map<string, json>,

    implements ai.MessagePart {
        function kind(self) -> ai.MessagePartKind throws never {
            ai.MessagePartKind.Audio
        }
        function media(self) -> image | audio | video | pdf | null throws never {
            self.value
        }
        function annotations(self) -> map<string, json> throws never {
            self.metadata
        }
    }
}

class VideoPart {
    value: video,
    metadata: map<string, json>,

    implements ai.MessagePart {
        function kind(self) -> ai.MessagePartKind throws never {
            ai.MessagePartKind.Video
        }
        function media(self) -> image | audio | video | pdf | null throws never {
            self.value
        }
        function annotations(self) -> map<string, json> throws never {
            self.metadata
        }
    }
}

class PdfPart {
    value: pdf,
    metadata: map<string, json>,

    implements ai.MessagePart {
        function kind(self) -> ai.MessagePartKind throws never {
            ai.MessagePartKind.Pdf
        }
        function media(self) -> image | audio | video | pdf | null throws never {
            self.value
        }
        function annotations(self) -> map<string, json> throws never {
            self.metadata
        }
    }
}

class ProviderDataPart {
    provider: string,
    data: json,

    implements ai.MessagePart {
        function kind(self) -> ai.MessagePartKind throws never {
            ai.MessagePartKind.ProviderData
        }
        function annotations(self) -> map<string, json> throws never {
            { "provider": self.provider, "data": self.data }
        }
    }
}
