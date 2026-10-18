class Dog { constructor(name, age) { this.name = name; this.age = age; } }
class Robot {
  constructor(serial, model) { this.serial = serial; this.model = model; }
  get name() { return this.model; }
}
function pick(i) { return i % 2 === 0 ? new Dog("rex", i) : new Robot(i, "r2"); }
function nameLen(n) { return n.name.length; }
function main() {
  let acc = 0;
  for (let i = 0; i < 100000; i++) acc += nameLen(pick(i));
  return acc;
}
