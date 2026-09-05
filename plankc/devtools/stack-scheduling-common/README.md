# Shared stack-scheduling devtool support

This library contains the corpus loading, SIR preparation, and database interchange types shared
by the stack-scheduling benchmark, database builder, database inspector, and submission tool.
It owns SQLite seeding, lookup, and conditional updates. Callers validate schedules before updating.
