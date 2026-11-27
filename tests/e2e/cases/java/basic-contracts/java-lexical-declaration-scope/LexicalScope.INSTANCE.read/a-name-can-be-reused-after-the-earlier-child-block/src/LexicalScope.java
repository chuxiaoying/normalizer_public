class LexicalScope {
    static final LexicalScope INSTANCE = new LexicalScope();
    int tally;
    int read(boolean choose, int input) {
        int tally = input;
        { int branch = tally + 1; } int branch = tally + 3; return branch;
    }
}
