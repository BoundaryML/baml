

// enum Categories {
//     ONE
//     TWO
// }
 
// function EnumFunc {
//     input Categories
//     output string
// } 
 
// impl<llm, EnumFunc> version {
//     client AZURE_GPT4
 
//     prompt #"
//         Given a user is trying to schedule a meeting, extract the relevant information
//         {#input}
//         information from the query.
//         JSON: 
//     "#
// } 

class BasicClass {
    name string
    age int
    address string
}
 
// function ClassFunc {
//     input BasicClass
//     output string
// }

// impl<llm, ClassFunc> version {
//     client AZURE_GPT4
 
//     prompt #"
//         Given a user is trying to schedule a meeting, extract the relevant information
//         {#input}
//         information from the query.
//         JSON:
//     "#
// }
   

 
// function Namedfunc {
//     input (name: BasicClass, address: string)
//     output string
// }
  
// impl<llm, Namedfunc> version {
//     client AZURE_GPT4
 
//     prompt #"
//         Given a userr is trying to schedule a meeting, extract the relevant information
//         {#input.name}
//         information from the query.
//         JSON:
//     "#
// } 

// function StringFunc {
//     input string
//     output string
// } 

// impl<llm, StringFunc> v1 {
//     client AZURE_GPT4
 
//     prompt #"
//         Given a userr is trying to schedule a meeting, extract the relevant information
//         {#input}
//         information from the query.
//         JSON:
//     "#
// }
 
// function IntFunc {
//     input int
//     output string
// }
// impl<llm, IntFunc> v1 {
//     client AZURE_GPT4
 
//     prompt #"
//         Given a userr is trying to schedule a meeting, extract the relevant information
//         {#input}
//         information from the query.
//         JSON:
//     "#
// }
 
// // booleanFunc
// function BooleanFunc {
//     input bool
//     output string
// }

// impl<llm, BooleanFunc> v1 {
//     client AZURE_GPT4

//     prompt #"
//         Given a user is trying to schedule a meeting, extract the relevant information
//         {#input}
//         information from the query.
//         JSON:
//     "#
// } 
 
// function OptionalNamedFunc {
//     input (name: BasicClass?, address: (string | null))
//     output string
// }

class OptionalObject {
    a string?
    b bool? 
}   
 
function OptionalFunc {
    input OptionalObject
    output string
}
 