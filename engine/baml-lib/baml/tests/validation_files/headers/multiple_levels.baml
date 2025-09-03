fn MultipleHeaderLevels() -> string {
    # Section One
    let a = "one";
    
    # Section Two
    let b = "two";
    
    ## Subsection 2.1
    let c = "three";
    
    ### Subsubsection 2.1.1
    let d = "four";
    
    # Section Three
    a + b + c + d
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  headers/mdx_headers_multiple_levels.baml:1
//    | 
//    | 
//  1 | function MultipleHeaderLevels() -> string {
//    | 


