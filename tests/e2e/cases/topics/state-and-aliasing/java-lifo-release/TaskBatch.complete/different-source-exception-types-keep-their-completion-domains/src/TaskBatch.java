record Task(int id) {}
class ExtraClose extends RuntimeException {}
class WrongTask extends RuntimeException {}
class TaskBatch {
    static boolean complete(Task[] opened, Task[] closed) {
        int n = opened.length;
        int m = closed.length;
        int mismatches;
        /* @normalizer-ir
           mismatches = count from [0, min(n, m)) as i
                        where closed[i].id != opened[n - 1 - i].id
        */
        if (m > n) throw new ExtraClose();
        if (mismatches > 0) throw new WrongTask();
        return m == n;
    }
}
