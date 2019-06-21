#[macro_use]
extern crate diesel;

use diesel::associations::HasTable;
use diesel::expression::operators::Eq;
use diesel::expression::{AsExpression, Expression};
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::query_builder::{self, AstPass, QueryFragment, QueryId};
use diesel::sql_types::HasSqlType;
use diesel::sql_types::*;
use diesel::QuerySource;

#[derive(Debug)]
pub struct KeysetPaginated<Query, Order, Cursor, CursorColumn> {
    pub query: Query,
    pub order: Order,
    pub cursor: Cursor,
    pub cursor_column: CursorColumn,
    pub page_size: i64,
}

impl<Query: QueryId, Order: QueryId, Cursor: 'static, CursorColumn: QueryId> QueryId
    for KeysetPaginated<Query, Order, Cursor, CursorColumn>
{
    type QueryId = KeysetPaginated<Query::QueryId, Order::QueryId, Cursor, CursorColumn::QueryId>;

    const HAS_STATIC_QUERY_ID: bool = Query::HAS_STATIC_QUERY_ID
        && Order::HAS_STATIC_QUERY_ID
        && CursorColumn::HAS_STATIC_QUERY_ID;
}

impl<Query: query_builder::Query, Order, Cursor, CursorColumn> query_builder::Query
    for KeysetPaginated<Query, Order, Cursor, CursorColumn>
{
    type SqlType = Query::SqlType;
}

impl<Query, Order, Cursor, CursorColumn> RunQueryDsl<PgConnection>
    for KeysetPaginated<Query, Order, Cursor, CursorColumn>
{
}

impl<Query, Order, Cursor, CursorColumn> QueryFragment<Pg>
    for KeysetPaginated<Query, Order, Cursor, CursorColumn>
where
    Query: QueryFragment<Pg>,
    Order: QueryFragment<Pg> + Expression,
    Pg: HasSqlType<Order::SqlType>,

    CursorColumn: Column,
    CursorColumn::Table: HasTable,
    <<CursorColumn::Table as HasTable>::Table as QuerySource>::FromClause: QueryFragment<Pg>,

    // This `Copy` is necessary because `.eq` moves `self`
    // Diesel columns always implement `Copy` to should be save
    CursorColumn: Expression + ExpressionMethods + Copy,
    // This `Clone` is necessary because `.as_expression` moves `self`
    // There might be a way to get around it, but I don't know how yet
    Cursor: AsExpression<CursorColumn::SqlType> + Clone,
    Eq<CursorColumn, <Cursor as AsExpression<<CursorColumn as Expression>::SqlType>>::Expression>:
        QueryFragment<Pg>,
{
    fn walk_ast(&self, mut out: AstPass<Pg>) -> QueryResult<()> {
        let table = <CursorColumn::Table as HasTable>::table();
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
        let where_clause = self.cursor_column.eq(self.cursor.clone().as_expression());
        where_clause.walk_ast(out.reborrow())?;
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
    use schema::*;

    mod schema {
        table! {
            users (id) {
                firstname -> Text,
                id -> Integer,
                lastname -> Text,
                slug -> Text,
            }
        }

        table! {
            follows (id) {
                followee_id -> Integer,
                followee_type -> Nullable<Text>,
                follower_id -> Integer,
                id -> Integer,
                source -> Nullable<Text>,
                unfollowed_at -> Nullable<Timestamptz>,
            }
        }

        allow_tables_to_appear_in_same_query!(follows, users);
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
            cursor: two.id,
            cursor_column: users::id,
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

    #[test]
    fn test_advanced_query() {
        let db = connect_to_db();

        let user = UserFactory::default().insert(&db);

        let query = follows::table
            .inner_join(users::table.on(users::id.eq(follows::followee_id)))
            .filter(
                follows::followee_type
                    .eq("User")
                    .and(follows::follower_id.eq(user.id))
                    .and(follows::unfollowed_at.is_null()),
            )
            .select(users::all_columns);

        let query = KeysetPaginated {
            query,
            order: (users::firstname, users::lastname),
            cursor_column: users::id,
            cursor: user.id,
            page_size: 20,
        };

        let users = query.load::<User>(&db).unwrap();

        assert!(users.is_empty());
    }

    fn connect_to_db() -> PgConnection {
        let url = "postgres://localhost/tonsser-api_test";
        let db = PgConnection::establish(url).unwrap();
        db.begin_test_transaction().unwrap();
        db
    }
}
