use minijinja::{value::Kwargs, ErrorKind, Value};

pub(crate) trait CallableJinja {
    type Params;

    fn params(&self) -> &[&'static str];

    fn parse_args(
        &self,
        state: &minijinja::State,
        args: &[minijinja::Value],
        kwargs: &mut Kwargs,
    ) -> Result<Self::Params, minijinja::Error>;

    // Call an actual function like {{ hello() }}
    fn call(
        &self,
        state: &minijinja::State,
        params: Self::Params,
    ) -> Result<minijinja::Value, minijinja::Error>;

    // Call a method on the object like {{ hello.there() }}
    fn call_method(
        &self,
        name: &str,
        state: &minijinja::State,
        args: &[minijinja::Value],
    ) -> Result<minijinja::Value, minijinja::Error> {
        let (args, mut kwargs): (&[Value], Kwargs) = minijinja::value::from_args(args)?;
        let params = self.parse_args(state, args, &mut kwargs)?;

        let Ok(_) = kwargs.assert_all_used() else {
            return Err(minijinja::Error::new(
                ErrorKind::TooManyArguments,
                format!(
                    "{name}() got an unexpected keyword argument (only {params} is allowed)",
                    name = name,
                    params = self.params().join(", "),
                ),
            ));
        };

        self.call(state, params)
    }
}
