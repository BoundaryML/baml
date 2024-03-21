class DynamicPropsClass {
  prop1 string
  prop2 string
  display string @get(typescript#"
    for (let i = 0; i < 10; i++) {
      console.log(i)
    }
    

    return this.prop1 + this.prop2
  "#, python#"
    for i in range(10):
      print(i)
    return self.prop1 + self.prop2
  "#)

  prop3 int
}

class SomeClass2 {
  prop1 string
  prop2 string
}