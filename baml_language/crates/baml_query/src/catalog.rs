use std::collections::HashMap;

use datafusion::arrow::datatypes::SchemaRef;

use crate::resident::SqliteResidentTableSpec;
use crate::{QueryError, Result};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RelationshipCardinality {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelationshipDefinition {
    pub from_table: String,
    pub from_columns: Vec<String>,
    pub to_table: String,
    pub to_columns: Vec<String>,
    pub cardinality: RelationshipCardinality,
    pub project_scoped: bool,
}

impl RelationshipDefinition {
    pub fn new(
        from_table: impl Into<String>,
        from_columns: Vec<String>,
        to_table: impl Into<String>,
        to_columns: Vec<String>,
        cardinality: RelationshipCardinality,
    ) -> Self {
        Self {
            from_table: from_table.into(),
            from_columns,
            to_table: to_table.into(),
            to_columns,
            cardinality,
            project_scoped: false,
        }
    }

    pub fn many_to_one(
        from_table: impl Into<String>,
        from_column: impl Into<String>,
        to_table: impl Into<String>,
        to_column: impl Into<String>,
    ) -> Self {
        Self::new(
            from_table,
            vec![from_column.into()],
            to_table,
            vec![to_column.into()],
            RelationshipCardinality::ManyToOne,
        )
    }

    pub fn one_to_many(
        from_table: impl Into<String>,
        from_column: impl Into<String>,
        to_table: impl Into<String>,
        to_column: impl Into<String>,
    ) -> Self {
        Self::new(
            from_table,
            vec![from_column.into()],
            to_table,
            vec![to_column.into()],
            RelationshipCardinality::OneToMany,
        )
    }

    #[must_use]
    pub fn project_scoped(mut self) -> Self {
        self.project_scoped = true;
        self
    }
}

#[derive(Clone, Debug)]
pub struct TableDefinition {
    pub name: String,
    pub schema: SchemaRef,
    pub project_column: Option<String>,
    hydrated_columns: Vec<String>,
}

impl TableDefinition {
    pub fn new(name: impl Into<String>, schema: SchemaRef) -> Result<Self> {
        let name = name.into();
        if name.is_empty() {
            return Err(QueryError::Internal(
                "table name cannot be empty".to_owned(),
            ));
        }
        Ok(Self {
            name,
            schema,
            project_column: None,
            hydrated_columns: Vec::new(),
        })
    }

    #[must_use]
    pub fn project_column(mut self, column: impl Into<String>) -> Self {
        self.project_column = Some(column.into());
        self
    }

    #[must_use]
    pub fn hydrated_column(mut self, column: impl Into<String>) -> Self {
        self.hydrated_columns.push(column.into());
        self
    }

    fn has_column(&self, column: &str) -> bool {
        self.schema.field_with_name(column).is_ok()
    }

    fn is_hydrated(&self, column: &str) -> bool {
        self.hydrated_columns.iter().any(|name| name == column)
    }
}

#[derive(Clone, Debug, Default)]
pub struct QueryCatalog {
    tables: HashMap<String, TableDefinition>,
    relationships: Vec<RelationshipDefinition>,
}

impl QueryCatalog {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_table(&mut self, table: TableDefinition) -> Result<()> {
        if self.tables.contains_key(&table.name) {
            return Err(QueryError::Internal(
                "duplicate query catalog table".to_owned(),
            ));
        }
        if let Some(project_column) = &table.project_column {
            if !table.has_column(project_column) {
                return Err(QueryError::Internal(format!(
                    "project column does not exist: {}.{}",
                    table.name, project_column
                )));
            }
        }
        self.tables.insert(table.name.clone(), table);
        Ok(())
    }

    pub fn register_sqlite_table(&mut self, table: &SqliteResidentTableSpec) -> Result<()> {
        self.register_table(table.table_definition())
    }

    pub fn register_relationship(&mut self, relationship: RelationshipDefinition) -> Result<()> {
        let from = self.table(&relationship.from_table)?;
        let to = self.table(&relationship.to_table)?;
        if relationship.from_columns.is_empty()
            || relationship.from_columns.len() != relationship.to_columns.len()
        {
            return Err(QueryError::Internal(
                "relationship sides must have the same non-zero number of columns".to_owned(),
            ));
        }
        for (from_column, to_column) in relationship
            .from_columns
            .iter()
            .zip(&relationship.to_columns)
        {
            let from_field = from.schema.field_with_name(from_column).map_err(|_| {
                QueryError::Internal(format!(
                    "relationship column does not exist: {}.{}",
                    relationship.from_table, from_column
                ))
            })?;
            let to_field = to.schema.field_with_name(to_column).map_err(|_| {
                QueryError::Internal(format!(
                    "relationship column does not exist: {}.{}",
                    relationship.to_table, to_column
                ))
            })?;
            if from_field.data_type() != to_field.data_type() {
                return Err(QueryError::Internal(format!(
                    "relationship column types differ: {}.{} ({}) and {}.{} ({})",
                    relationship.from_table,
                    from_column,
                    from_field.data_type(),
                    relationship.to_table,
                    to_column,
                    to_field.data_type()
                )));
            }
            if from.is_hydrated(from_column) || to.is_hydrated(to_column) {
                return Err(QueryError::Internal(
                    "hydrated value columns cannot be relationship keys".to_owned(),
                ));
            }
        }
        if relationship.project_scoped
            && (from.project_column.is_none() || to.project_column.is_none())
        {
            return Err(QueryError::Internal(
                "project-scoped relationships require project columns on both tables".to_owned(),
            ));
        }
        self.relationships.push(relationship);
        Ok(())
    }

    pub fn table(&self, name: &str) -> Result<&TableDefinition> {
        self.tables
            .get(name)
            .ok_or_else(|| QueryError::Internal(format!("unknown query catalog table: {name}")))
    }

    pub fn tables(&self) -> impl Iterator<Item = &TableDefinition> {
        self.tables.values()
    }

    #[must_use]
    pub fn relationships(&self) -> &[RelationshipDefinition] {
        &self.relationships
    }
}
