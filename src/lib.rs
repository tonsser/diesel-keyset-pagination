#[macro_use]
extern crate diesel;

use diesel::associations::HasTable;
use diesel::expression::{AsExpression, Expression};
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::query_builder::{self, AstPass, QueryFragment, QueryId};
use diesel::sql_types::HasSqlType;
use diesel::sql_types::*;
use diesel::QuerySource;

#[derive(Debug)]
pub struct KeysetPaginated<Query, Order, Filter> {
    pub query: Query,
    pub order: Order,
    pub filter: Filter,
    pub page_size: i64,
}

impl<Query: QueryId, Order: QueryId, Filter: 'static> QueryId
    for KeysetPaginated<Query, Order, Filter>
{
    type QueryId = KeysetPaginated<Query::QueryId, Order::QueryId, Filter>;

    const HAS_STATIC_QUERY_ID: bool = Query::HAS_STATIC_QUERY_ID && Order::HAS_STATIC_QUERY_ID;
}

impl<Query: query_builder::Query, Order, Filter> query_builder::Query
    for KeysetPaginated<Query, Order, Filter>
{
    type SqlType = Query::SqlType;
}

impl<Query, Order, Filter> RunQueryDsl<PgConnection> for KeysetPaginated<Query, Order, Filter> {}

impl<Query, Order, Filter> QueryFragment<Pg> for KeysetPaginated<Query, Order, Filter>
where
    Query: QueryFragment<Pg> + HasTable,
    Query::Table: HasTable,
    <<Query::Table as HasTable>::Table as QuerySource>::FromClause: QueryFragment<Pg>,
    Order: QueryFragment<Pg> + Expression,
    Pg: HasSqlType<Order::SqlType>,
    Filter: AsExpression<Bool> + QueryFragment<Pg>,
{
    fn walk_ast(&self, mut out: AstPass<Pg>) -> QueryResult<()> {
        let table = <<Query as HasTable>::Table as HasTable>::table();
        let from_clause = table.from_clause();

        out.push_sql("SELECT * FROM (");
        self.query.walk_ast(out.reborrow())?;
        out.push_sql(") ");
        from_clause.walk_ast(out.reborrow())?;

        out.push_sql(" WHERE ");
        out.push_sql("(");
        self.order.walk_ast(out.reborrow())?;
        out.push_sql(")");
        out.push_sql(" > ");
        out.push_sql("(");
        out.push_sql("SELECT ");
        self.order.walk_ast(out.reborrow())?;
        out.push_sql(" FROM ");
        from_clause.walk_ast(out.reborrow())?;
        out.push_sql(" WHERE ");
        self.filter.walk_ast(out.reborrow())?;
        out.push_sql(")");

        out.push_sql(" ORDER BY ");
        self.order.walk_ast(out.reborrow())?;

        out.push_sql(" LIMIT ");
        out.push_bind_param::<BigInt, _>(&self.page_size)?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    #[allow(unused_imports)]
    use super::*;
    use diesel_factories::{sequence, Factory};
    use schema::users;

    mod schema {
        table! {
            users (id) {
                firstname -> Text,
                id -> Integer,
                lastname -> Text,
                slug -> Text,
            }
        }
    }

    #[derive(Eq, PartialEq, Debug, Clone, QueryableByName, Queryable, Identifiable)]
    #[table_name = "users"]
    pub struct User {
        pub firstname: String,
        pub id: i32,
        pub lastname: String,
        pub slug: String,
    }

    #[derive(Factory, Clone)]
    #[factory(model = "User", table = "schema::users", connection = "PgConnection")]
    pub struct UserFactory {
        firstname: String,
        lastname: String,
        slug: String,
    }

    impl Default for UserFactory {
        fn default() -> Self {
            UserFactory {
                slug: sequence(|n| format!("bob-{}", n)),
                firstname: sequence(|n| format!("Bob {}", n)),
                lastname: sequence(|n| format!("Larsen {}", n)),
            }
        }
    }

    #[test]
    #[allow(unused_variables)]
    fn test_it() {
        use schema::users;

        let url = "postgres://localhost/tonsser-api_test";
        let db = PgConnection::establish(url).unwrap();
        db.begin_test_transaction().unwrap();

        let one = UserFactory::default()
            .firstname("one")
            .slug("a")
            .insert(&db);
        let two = UserFactory::default()
            .firstname("two")
            .slug("ab")
            .insert(&db);
        let three = UserFactory::default()
            .firstname("three")
            .slug("abc")
            .insert(&db);
        let four = UserFactory::default()
            .firstname("four")
            .slug("abcd")
            .insert(&db);
        let five = UserFactory::default()
            .firstname("five")
            .slug("abdce")
            .insert(&db);

        let query = KeysetPaginated {
            query: users::table.select(users::all_columns),
            order: (users::slug, users::id),
            filter: users::id.eq(two.id),
            page_size: 2,
        };

        let sql = diesel::debug_query::<Pg, _>(&query).to_string();
        eprintln!("{}\n\n", sql);

        let users = query.load::<User>(&db).unwrap();

        assert_eq!(
            users
                .into_iter()
                .map(|user| user.firstname)
                .collect::<Vec<_>>(),
            vec![three.firstname, four.firstname],
        );
    }
}
