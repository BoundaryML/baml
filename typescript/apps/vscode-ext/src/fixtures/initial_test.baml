// Enum 
enum Sentiment {
  SAD
  HAPPY 
}

enum Sentiment2 {
  // comment
  ANGRY @alias("angry")
  @meta(describe, "this is a description")
  @meta(keywords, "this is a keyword")
}
 
// Interface blocks 
class Interface1 {
  prop String
  prop2 Int
  propArray String[]
  boolean Bool
}

class InterfaceTwo {
  interface1 Interface1

  // comment
  prop2 String
}


function FunctionOne {
  @@attribute2({
    key "value"
    key2 "value2"})
  @@attribute2({
    key "value"
    key2 "value2"
    nestedKey {
      moreNesting "value2"
    }
  })

  input Interface1
  // comment
  output InterfaceTwo
  
}

function FunctionTwo {
  input Interface1
  output String
}

function Function3 {
  input {
    key1 String
    key2 Int
    keyBool Bool
  }
  output {
    key1 String
    key2 Int
    keyBool Bool
  }
}





generator Default {
  language "python"
  output "../path/to/dir"
}


client<llm> SomeOtherClient {
  
  model gpt-3-test
  my_custom_var "some string with spaces"
  othervars "sefoijsefoi, osijfosie"
  dictionary_key {
    array_key [ "abc", "abc", aaa    ],
    key2 "some random value", hello "world", thing "hello"
    block_string #"hello there my frien
                sefs d"#
    inline_raw_string #"inline "raw" string,,"#
    keyStringNoSpacesWorksWithoutQuotes NoQuotes!!123,
    valueWithSpacesIllegal hello there my friend
    "key with" "is this value123",
    key3 ../this-is-also-a-string,
    booleanValue true,
    stringKey "true"
    array1 [
      "one"
      "two"
    ]
    array2 [ "one", "two" ]
    array3Numbers [ 1, 2, 3 ]
    nullVal null
    nestedKey {
      key "value"
      key2 "value2"
      nestedArray [ "yes", "queen"]
      number 10
    }
    oneMoreValueToTestBrokenness "value"
  }
  myInteger 100
  block_string #"
    hello there my friend
    {#//a comment in a prompt}

    {#variable.thing} {#//these are only valid of #}
  "#
}



function FooBar {
  input Int[] | String
  output (Int | string[])[]
}

function Three {
  input { hello Int? | String, world Int | String }
  output Int[]? // should error out by lang server, we only accept empty lists
}

impl<llm, FooBar> SomeName {
  serializer MyType {
    @@attribute1("hello")
    @@attribute2({
      key "value"
      key2 "value2"
    })

    property1
		@alias
		@skip

		@describe()
		@keywords()

		VALUE_2
		@describe()

		custom_prop @enum.dynamic_string({
			template (
				{{name}}: {{describe}} {{keywords}}
			)
			delimiter \n
		})

    serializer MyTypeA {
      fieldA
      @describe()

      fieldB
      @describe()

      full_name @dynamic_string({
        template (
          {{self.fieldA}} {{self.fieldB}}
        )
      })
    }
   
	} 

  client SomeOtherClient
  prompt #"
    does something
    some more text
    {#//a comment in a prompt}

    {}
  "#
}

generator Default {
  language python
  output ../path/to/dir
  features {
    openai true
    anthropic true
    fast true
    slow true
  }
}


class Conversation {
    thread Message[]

    as_str string @dynamic_string((
				Conversation:
				{{% for message in thread %}}
				{{message.as_str}}
				{{% endfor %}}
		))
    most_recent_message string @dynamic({
			python (
				@property
        def most_recent_message(self) -> str:
            if len(self.thread) <= 0: return ""
            return self.thread[-1].as_str
			)
		})

    historical_messages string @dynamic(python#"
            @property
            def historical_messages(self) -> str:
                if len(self.thread) <= 1: return ""
                history = '\n'.join(map(lambda m: m.as_str, self.thread[:-1]))
                return f"Prior Conversation:\n{history}"
        "#)
}