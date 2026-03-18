package commons;


import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

public class BudgetTest {

    private Budget testBudget1;
    private Budget testBudget2;
    private Budget testBudget3;

    private final ExpenseCategory testCategory = new ExpenseCategory("food");
    private final User testUser = new User();

    @BeforeEach
    void setUp() {
        testBudget1 = new Budget(50.0, 11, 2000, testCategory);
        testBudget2 = new Budget(100.0, 4, 2010, testCategory);
        testBudget3 = new Budget(100.0, 4, 2010, testCategory);
    }

    @Test
    void invalidConstructorTest() {
        assertThrows(IllegalArgumentException.class, () -> {
           Budget invalidBudget = new Budget(-1.0, 10, 2000, testCategory);
        }, "invalid limit");

        assertThrows(IllegalArgumentException.class, () -> {
           Budget invalidBudget = new Budget(1.0, 13, 2000, testCategory);
        }, "invalid month");

        assertThrows(IllegalArgumentException.class, () -> {
           Budget invalidBudget = new Budget(1.0, 10, 10000, testCategory);
        }, "invalid year");

        assertThrows(IllegalArgumentException.class, () -> {
           Budget invalidBudget = new Budget(1.0, 10, 2000, null);
        }, "not null category");
    }

    @Test
    void setIdTest() {
        Long id = 1L;
        testBudget1.setId(id);
        assertEquals(id, testBudget1.getId());
    }

    @Test
    void setUserTest() {
        testBudget1.setUser(testUser);
        assertEquals(testUser, testBudget1.getUser());
    }

    @Test
    void setValidLimitAmountTest() {
        double limit = 100.0;
        testBudget1.setLimitAmount(limit);
        assertEquals(limit, testBudget1.getLimitAmount());
    }

    @Test
    void setInvalidLimitAmountTest() {
        double limit = -100.0;
        assertThrows(IllegalArgumentException.class, () -> testBudget1.setLimitAmount(limit));
    }

    @Test
    void setValidMonthTest() {
        int month = 5;
        testBudget1.setMonth(month);
        assertEquals(month, testBudget1.getMonth());
    }

    @Test
    void setInvalidMonthTest() {
        int month = 13;
        assertThrows(IllegalArgumentException.class, () -> testBudget1.setMonth(month));
    }

    @Test
    void setValidYearTest() {
        int year = 1950;
        testBudget1.setYear(year);
        assertEquals(year, testBudget1.getYear());
    }

    @Test
    void setInvalidYearTest() {
        int year = 1900;
        assertThrows(IllegalArgumentException.class, () -> testBudget1.setYear(year));
    }

    @Test
    void setNullCategory() {
        ExpenseCategory newCategory = new ExpenseCategory("travel");
        testBudget1.setCategory(newCategory);
        assertEquals(newCategory, testBudget1.getCategory());
    }

    @Test
    void setNotNullCategory() {
        assertThrows(IllegalArgumentException.class, () -> {
            testBudget1.setCategory(null);
        });
    }

    @Test
    void testNotEquals() {
        assertNotEquals(testBudget1, testBudget2);
    }

    @Test
    void testEquals() {
        assertEquals(testBudget3, testBudget2);
    }

    @Test
    void testHashCodeDiff() {
        assertNotEquals(testBudget1.hashCode(), testBudget2.hashCode());
    }

    @Test
    void testHashCodeSame() {
        assertEquals(testBudget2.hashCode(), testBudget3.hashCode());
    }

    @Test
    void testToString() {
        String expected = "Budget{id=null, limitAmount=100.0, month=4, year=2010, category=" + testCategory + "}";
        assertEquals(expected, testBudget2.toString());
    }
}
