function f(dog_t: reflect.Type, animal_t: reflect.Type) -> bool {
    let views = dog_t.class.enum.function.interface;
    dog_t.implements(animal_t)
}
