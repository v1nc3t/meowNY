package com.meowny.server.dto.recurringtransaction;

import com.fasterxml.jackson.databind.annotation.JsonDeserialize;
import com.meowny.server.entity.Frequency;
import com.meowny.server.entity.TransactionType;
import com.meowny.server.config.HtmlSanitizationDeserializer;
import jakarta.validation.constraints.*;

import java.math.BigDecimal;
import java.time.LocalDate;

public record CreateRecurringTransactionRequest(
        @NotNull(message = "User ID is required")
        Long userId,

        @NotNull(message = "Category ID is required")
        Long categoryId,

        @NotNull(message = "Transaction type is required")
        TransactionType type,

        @NotBlank(message = "Transaction name is required")
        @Size(max = 50, message = "Transaction name must be 50 characters or fewer")
        @JsonDeserialize(using = HtmlSanitizationDeserializer.class)
        String name,

        @NotNull(message = "Amount is required")
        @PositiveOrZero(message = "Amount must be zero or positive")
        @Digits(integer = 12, fraction = 2)
        BigDecimal amount,

        @NotNull(message = "Next due date is required")
        LocalDate nextDueDate,

        @NotNull(message = "Frequency is required")
        Frequency frequency
) {
}
