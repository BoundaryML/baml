// Test: Type narrowing for unions under OR conditions

class Cat {
  type "cat"
  name string
}

class Dog {
  type "dog"
  name string
  bark_volume int
}

class Ball {
  type "ball"
  radius int
}

type Thing = Cat | Dog | Ball

// Test 1: Simple narrowing - should work, accessing 'name' on Cat|Dog
template_string Test1(thing: Thing) #"
{% if thing.type == "cat" or thing.type == "dog" %}
  {{ thing.name }}
{% endif %}
"#

// Test 2: Narrowing with field not on all - should warn about Cat missing bark_volume
template_string Test2(thing: Thing) #"
{% if thing.type == "cat" or thing.type == "dog" %}
  {{ thing.bark_volume }}
{% endif %}
"#

// Test 3: Sequential ifs - thing should reset to Thing after first if
template_string Test3(thing: Thing) #"
{% if thing.type == "cat" %}
  Cat!
{% endif %}
{% if thing.type == "dog" %}
  Dog!
{% endif %}
"#

// Test 4: Else block has complementary implication.
// We would like to not get a warning here, but the
// predicate analysis doesn't go far enough to get
// negative implications like this.
template_string Test4(thing: Thing) #"
{% if thing.type == "cat" or thing.type == "dog" %}
  Animal!
{% else %}
  Ball with radius {{ thing.radius }}!
{% endif %}
"#

// warning: property 'bark_volume' does not exist on Cat
//   -->  template_string/union_type_narrowing.baml:31
//    | 
// 30 | {% if thing.type == "cat" or thing.type == "dog" %}
// 31 |   {{ thing.bark_volume }}
//    | 
// warning: property 'radius' does not exist on Dog, Cat in type alias Thing
//   -->  template_string/union_type_narrowing.baml:53
//    | 
// 52 | {% else %}
// 53 |   Ball with radius {{ thing.radius }}!
//    | 
