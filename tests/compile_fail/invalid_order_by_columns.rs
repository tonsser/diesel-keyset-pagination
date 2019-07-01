#![allow(unused_imports)]

#[macro_use]
extern crate diesel;

use chrono::prelude::*;
use diesel_keyset_pagination::*;
use diesel::prelude::*;
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
}

#[derive(Eq, PartialEq, Debug, Clone, Queryable)]
pub struct User {
    pub firstname: String,
    pub id: i32,
    pub lastname: String,
    pub slug: String,
}

fn main() {
    let url = "postgres://localhost/tonsser-api_test";
    let db = PgConnection::establish(url).unwrap();

    users::table
        .select(users::all_columns)
        .keyset_paginate_order_by(follows::id)
        .page_size(2)
        .cursor(users::id, None::<i32>)
        .load::<User>(&db)
        .unwrap();
}
