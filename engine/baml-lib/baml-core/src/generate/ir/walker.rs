use super::{
    repr::{self, FunctionConfig},
    Class, Client, Enum, Function, Impl, RetryPolicy, TestCase, Walker,
};

impl<'a> Walker<'a, &'a Function> {
    pub fn name(&self) -> &str {
        self.elem().name()
    }

    pub fn walk_impls(
        &'a self,
    ) -> either::Either<
        impl Iterator<Item = Walker<'a, (&'a Function, &'a Impl)>>,
        impl Iterator<Item = Walker<'a, (&'a Function, &'a FunctionConfig)>>,
    > {
        match &self.item.elem {
            repr::Function::V1(f) => either::Either::Left(f.impls.iter().map(|i| Walker {
                db: self.db,
                item: (self.item, i),
            })),
            repr::Function::V2(f) => either::Either::Right(f.configs.iter().map(|c| Walker {
                db: self.db,
                item: (self.item, c),
            })),
        }
    }

    pub fn walk_tests(&'a self) -> impl Iterator<Item = Walker<'a, (&'a Function, &'a TestCase)>> {
        self.tests().iter().map(|i| Walker {
            db: self.db,
            item: (self.item, i),
        })
    }

    fn tests(&self) -> &'a Vec<TestCase> {
        match &self.item.elem {
            repr::Function::V1(f) => &f.tests,
            repr::Function::V2(f) => &f.tests,
        }
    }

    pub fn elem(&self) -> &'a repr::Function {
        &self.item.elem
    }

    pub fn output(&self) -> &'a repr::FieldType {
        match &self.item.elem {
            repr::Function::V1(f) => &f.output.elem,
            repr::Function::V2(f) => &f.output.elem,
        }
    }

    pub fn inputs(
        &self,
    ) -> either::Either<&'a repr::FunctionArgs, &'a Vec<(String, repr::FieldType)>> {
        self.item.elem.inputs()
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

impl<'a> Walker<'a, &'a RetryPolicy> {
    pub fn elem(&self) -> &'a repr::RetryPolicy {
        &self.item.elem
    }
}
