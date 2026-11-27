class NestedLoop {
    static int count(int rows, int columns) {
        int total = 0;
        for (int row = 0; row <= rows; row++) {
            for (int column = 0; column < columns; column++) total = total + 1;
        }
        return total;
    }
}
