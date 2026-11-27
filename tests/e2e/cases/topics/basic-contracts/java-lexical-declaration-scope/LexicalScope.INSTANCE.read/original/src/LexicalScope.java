class LexicalScope {
    static final LexicalScope INSTANCE = new LexicalScope();
    int tally;
    int read(boolean choose, int input) {
        int tally = input;
        if (choose) {
            int branch = tally + 1;
            return branch;
        } else {
            int branch = tally + 2;
            return branch;
        }
    }
}
