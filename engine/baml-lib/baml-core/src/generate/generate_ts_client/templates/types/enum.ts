interface IFoo {
  property: string;
  get property2(): string;
}

class Foo implements IFoo {
  constructor(items?: IFoo) {
    if (items) {
      Object.assign(this, items);
    }
  }

  public property: string;
  public get property2(): string {
    return this.property;
  }
}
