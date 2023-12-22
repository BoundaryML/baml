
class ZenfetchBotDocumentBase {
    title string?
    topic string?

    display string @get(
        python#"
            return f""" 
            #### Document:
                - Title: {self.title}
                - Topic: {self.topic}
                - Text: {self.text}
                - Author: {self.author}
                - URL: {self.raw_url}
                - Date Created: {self.date_created}
            """.strip()
        "#
    )
}


class ZenfetchBotDocumentBaseList {
    list_of_documents ZenfetchBotDocumentBase[]
    is_empty bool

    display string @get(
        python#"
            ret = []
            if self.list_of_documents:
                for doc in list_of_documents:
                    ret.append(doc.display)
            return "\n".join(ret)
        "#
    )
}
 
 
function GenerateUserChatPrompts {
    input ZenfetchBotDocumentBaseList
    output string
}
impl<llm, GenerateUserChatPrompts> version {
    client AZURE_GPT4
 
    prompt #"
        Given a user is trying to schedule a meeting, extract the relevant information
        {#input}
        information from the query.
        JSON:
    "#
}

enum Categories {
    ONE
    TWO
}

function EnumFunc {
    input Categories
    output string
} 
 
impl<llm, EnumFunc> version {
    client AZURE_GPT4
 
    prompt #"
        Given a user is trying to schedule a meeting, extract the relevant information
        {#input}
        information from the query.
        JSON:
    "#
}

class BasicClass {
    name string
    age int
    address string
}
 
function ClassFunc {
    input BasicClass
    output string
}

impl<llm, ClassFunc> version {
    client AZURE_GPT4
 
    prompt #"
        Given a user is trying to schedule a meeting, extract the relevant information
        {#input}
        information from the query.
        JSON:
    "#
}
   

 
function Namedfunc {
    input (name: BasicClass, address: string)
    output string
}
  
impl<llm, Namedfunc> version {
    client AZURE_GPT4
 
    prompt #"
        Given a userr is trying to schedule a meeting, extract the relevant information
        {#input.name}
        information from the query.
        JSON:
    "#
} 

function StringFunc {
    input string
    output string
} 

impl<llm, StringFunc> v1 {
    client AZURE_GPT4
 
    prompt #"
        Given a userr is trying to schedule a meeting, extract the relevant information
        {#input}
        information from the query.
        JSON:
    "#
}
 
function IntFunc {
    input int
    output string
}
impl<llm, IntFunc> v1 {
    client AZURE_GPT4
 
    prompt #"
        Given a userr is trying to schedule a meeting, extract the relevant information
        {#input}
        information from the query.
        JSON:
    "#
}
 
// booleanFunc
function BooleanFunc {
    input bool
    output string
}

impl<llm, BooleanFunc> v1 {
    client AZURE_GPT4
 
    prompt #"
        Given a userr is trying to schedule a meeting, extract the relevant information
        {#input}
        information from the query.
        JSON:
    "#
}

 