record Task(int id) {} record Completed(int id) {}
class TaskBatch {
    static boolean complete(Task[] opened, Completed[] closed) {
        int n = opened.length;
        int m = closed.length;
        int mismatches;
        /* @normalizer-ir
           mismatches = count from [0, min(n, m)) as i
                        where closed[i].id != opened[n - 1 - i].id
        */
        if (m > n) throw new IllegalArgumentException();
        if (mismatches > 0) throw new IllegalArgumentException();
        return m == n;
    }
}
