package baml_go

import (
	"encoding/json"
	"testing"

	"github.com/boundaryml/baml-go/internal/cffi"
	"google.golang.org/protobuf/proto"
)

func TestPromptPortablePayloadCanBeDecodedPersistedAndReentered(t *testing.T) {
	outbound := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_PromptAstValue{
		PromptAstValue: &cffi.BamlValuePromptAst{Value: &cffi.BamlValuePromptAst_Simple{
			Simple: &cffi.BamlValuePromptAstSimple{Value: &cffi.BamlValuePromptAstSimple_String_{String_: "hello"}},
		}},
	}}}
	prompt, err := outbound.Prompt()
	if err != nil {
		t.Fatal(err)
	}
	persisted, err := json.Marshal(prompt)
	if err != nil {
		t.Fatal(err)
	}
	var restored Prompt
	if err := json.Unmarshal(persisted, &restored); err != nil {
		t.Fatal(err)
	}

	payload, err := encodeCall(7, map[string]Input{"prompt": restored.BAMLInput()})
	if err != nil {
		t.Fatal(err)
	}
	var call cffi.CallFunctionArgs
	if err := proto.Unmarshal(payload, &call); err != nil {
		t.Fatal(err)
	}
	got := call.Kwargs[0].Value.GetPromptAstValue().GetSimple().GetString_()
	if got != "hello" || call.Kwargs[0].Value.GetHandle() != nil {
		t.Fatalf("portable prompt = %q / handle %#v", got, call.Kwargs[0].Value.GetHandle())
	}
}
