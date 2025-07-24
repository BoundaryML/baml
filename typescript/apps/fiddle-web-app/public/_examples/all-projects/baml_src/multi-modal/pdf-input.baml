// BAML supports multi-modal inputs including Pdfs!

// How to call this BAML function in python or Typescript: https://docs.boundaryml.com/docs/snippets/supported-types#pdf

function PdfInput(pdfInput: pdf) -> string {
  client GPT4o
  prompt #"
    {{ _.role("user") }}

    Get the title of the pdfs
    
    {{ pdfInput }}
  "#
}

test TestFilePdfInput {
  functions [PdfInput]
  args {
    pdfInput { 
      url "https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf"
      media_type "application/pdf"
    }
  }  
} 