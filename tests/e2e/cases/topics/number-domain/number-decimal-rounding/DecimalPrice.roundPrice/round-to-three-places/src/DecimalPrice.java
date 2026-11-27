import java.math.BigDecimal;
import normalizer.intrinsics.N;

class DecimalPrice {
    static BigDecimal roundPrice(BigDecimal value) {
        return N.round(value, 3, N.HALF_UP);
    }
}
