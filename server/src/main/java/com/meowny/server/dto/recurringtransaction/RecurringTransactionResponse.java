package com.meowny.server.dto.recurringtransaction;

import com.meowny.server.entity.Frequency;
import com.meowny.server.entity.TransactionType;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.time.LocalDateTime;

public record RecurringTransactionResponse(
        Long id,
        Long userId,
        Long categoryId,
        String categoryName,
        TransactionType type,
        String name,
        BigDecimal amount,
        LocalDate nextDueDate,
        Frequency frequency,
        boolean isActive,
        LocalDateTime createdAt,
        LocalDateTime updatedAt
) {
}
