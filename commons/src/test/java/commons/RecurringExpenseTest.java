package commons;

import org.junit.jupiter.api.*;

import java.time.LocalDate;

import static org.junit.jupiter.api.Assertions.*;

public class RecurringExpenseTest {

    private RecurringExpense testRecurringExpense1;
    private RecurringExpense testRecurringExpense2;
    private RecurringExpense testRecurringExpense3;

    private final LocalDate testDate = LocalDate.of(10000, 1, 1);
    private final Frequency testFrequency = Frequency.WEEKLY;
    private final User testUser = new User();
    private final ExpenseCategory testCategory = new ExpenseCategory("bills");

    @BeforeEach
    void setUp() {
        testRecurringExpense1 = new RecurringExpense(
                "rent", 600.0, testDate, testFrequency, testUser, testCategory
        );
        testRecurringExpense2 = new RecurringExpense(
                "car", 200.0, testDate, testFrequency, testUser, testCategory
        );
        testRecurringExpense3 = new RecurringExpense(
                "car", 200., testDate, testFrequency, testUser, testCategory
        );
    }

    @Test
    void invalidConstructorTest() {
        assertThrows(IllegalArgumentException.class, () -> {
            RecurringExpense testRecurringExpense = new RecurringExpense(
                    null, 600.0, testDate, testFrequency, testUser, testCategory
            );
        }, "name null");

        assertThrows(IllegalArgumentException.class, () -> {
            RecurringExpense testRecurringExpense = new RecurringExpense(
                    "rent", null, testDate, testFrequency, testUser, testCategory
            );
        }, "amount null");

        assertThrows(IllegalArgumentException.class, () -> {
            RecurringExpense testRecurringExpense = new RecurringExpense(
                    "rent", -600.0, testDate, testFrequency, testUser, testCategory
            );
        }, "amount negative");

        assertThrows(IllegalArgumentException.class, () -> {
            RecurringExpense testRecurringExpense = new RecurringExpense(
                    "rent", 600.0, null, testFrequency, testUser, testCategory
            );
        }, "next due date null");

        assertThrows(IllegalArgumentException.class, () -> {
            RecurringExpense testRecurringExpense = new RecurringExpense(
                    "rent", 600.0, testDate, null, testUser, testCategory
            );
        }, "frequency null");

        assertThrows(IllegalArgumentException.class, () -> {
            RecurringExpense testRecurringExpense = new RecurringExpense(
                    "rent", 600.0, testDate, testFrequency, null, testCategory
            );
        }, "user null");

        assertThrows(IllegalArgumentException.class, () -> {
            RecurringExpense testRecurringExpense = new RecurringExpense(
                    "rent", 600.0, testDate, testFrequency, testUser, null
            );
        }, "category null");
    }

    @Test
    void setIdTest() {
        Long id = 1L;
        testRecurringExpense1.setId(id);
        assertEquals(id, testRecurringExpense1.getId());
    }

    @Test
    void setValidNameTest() {
        String name = "test";
        testRecurringExpense1.setName(name);
        assertEquals(name, testRecurringExpense1.getName());
    }

    @Test
    void setInvalidNameTest() {
        assertThrows(IllegalArgumentException.class, () -> {
            testRecurringExpense1.setName(null);
        });
    }

    @Test
    void setValidAmountTest() {
        Double amount = 20.0;
        testRecurringExpense1.setAmount(amount);
        assertEquals(amount, testRecurringExpense1.getAmount());
    }

    @Test
    void setNullAmountTest() {
        assertThrows(IllegalArgumentException.class, () -> {
            testRecurringExpense1.setAmount(null);
        });
    }

    @Test
    void setNegativeAmountTest() {
        assertThrows(IllegalArgumentException.class, () -> {
            testRecurringExpense1.setAmount(-2.0);
        });
    }

    @Test
    void setValidDueDateTest() {
        LocalDate date = LocalDate.of(2026, 5, 5);
        testRecurringExpense1.setNextDueDate(date);
        assertEquals(date, testRecurringExpense1.getNextDueDate());
    }

    @Test
    void setNullDueDateTest() {
        assertThrows(IllegalArgumentException.class, () -> {
            testRecurringExpense1.setNextDueDate(null);
        });
    }

    @Test
    void setBeforeDueDateTest() {
        LocalDate date = LocalDate.of(2000, 5, 5);
        assertThrows(IllegalArgumentException.class, () -> {
            testRecurringExpense1.setNextDueDate(date);
        });
    }

    @Test
    void setValidFrequencyTest() {
        Frequency frequency = Frequency.DAILY;
        testRecurringExpense1.setFrequency(frequency);
        assertEquals(frequency, testRecurringExpense1.getFrequency());
    }

    @Test
    void setInvalidFrequencyTest() {
        assertThrows(IllegalArgumentException.class, () -> {
            testRecurringExpense1.setFrequency(null);
        });
    }

    @Test
    void setValidUserTest() {
        User user = new User();
        testRecurringExpense1.setUser(user);
        assertEquals(user, testRecurringExpense1.getUser());
    }

    @Test
    void setInvalidUserTest() {
        assertThrows(IllegalArgumentException.class, () -> {
            testRecurringExpense1.setUser(null);
        });
    }

    @Test
    void setValidTest() {
        ExpenseCategory category = new ExpenseCategory("car");
        testRecurringExpense1.setCategory(category);
        assertEquals(category, testRecurringExpense1.getCategory());
    }

    @Test
    void setInvalidTest() {
        assertThrows(IllegalArgumentException.class, () -> {
            testRecurringExpense1.setCategory(null);
        });
    }

    @Test
    void notEqualsTest() {
        assertNotEquals(testRecurringExpense1, testRecurringExpense2);
    }

    @Test
    void equalsTest() {
        assertEquals(testRecurringExpense3, testRecurringExpense2);
    }

    @Test
    void hashCodeDiffTest() {
        assertNotEquals(testRecurringExpense1.hashCode(), testRecurringExpense2.hashCode());
    }

    @Test
    void hashCodeSameTest() {
        assertEquals(testRecurringExpense3.hashCode(), testRecurringExpense2.hashCode());
    }

}
