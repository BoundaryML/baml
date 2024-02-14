use super::{
    repr::{self},
    Class, Client, Enum, Function, Impl, Walker,
};

impl<'a> Walker<'a, &'a Function> {
    pub fn walk_impls(&'a self) -> impl Iterator<Item = Walker<'a, (&'a Function, &'a Impl)>> {
        self.item.elem.impls.iter().map(|i| Walker {
            db: self.db,
            item: (self.item, i),
        })
    }

    pub fn elem(&self) -> &'a repr::Function {
        &self.item.elem
    }
}

impl<'a> Walker<'a, &'a Enum> {
    #[allow(dead_code)]
    pub fn walk_values(&'a self) -> impl Iterator<Item = &'a repr::EnumValue> {
        self.item.elem.values.iter().map(|v| &v.elem)
    }

    pub fn elem(&self) -> &'a repr::Enum {
        &self.item.elem
    }
}

impl<'a> Walker<'a, (&'a Function, &'a Impl)> {
    #[allow(dead_code)]
    pub fn function(&'a self) -> Walker<'a, &'a Function> {
        Walker {
            db: self.db,
            item: self.item.0,
        }
    }

    pub fn elem(&self) -> &'a repr::Implementation {
        &self.item.1.elem
    }
}

impl<'a> Walker<'a, &'a Class> {
    #[allow(dead_code)]
    pub fn walk_fields(&'a self) -> impl Iterator<Item = &'a repr::Field> {
        self.item.elem.static_fields.iter().map(|f| &f.elem)
    }

    pub fn elem(&self) -> &'a repr::Class {
        &self.item.elem
    }
}

impl<'a> Walker<'a, &'a Client> {
    pub fn elem(&self) -> &'a repr::Client {
        &self.item.elem
    }
}
