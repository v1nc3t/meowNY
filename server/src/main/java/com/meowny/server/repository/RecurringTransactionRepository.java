package com.meowny.server.repository;

import com.meowny.commons.entity.RecurringTransaction;
import jakarta.persistence.LockModeType;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.jpa.repository.Lock;
import org.springframework.data.jpa.repository.Query;
import org.springframework.data.repository.query.Param;

import java.time.LocalDate;
import java.util.List;

public interface RecurringTransactionRepository extends JpaRepository<RecurringTransaction, Long> {

    List<RecurringTransaction> findByUserIdAndIsActiveTrue(Long userId);

    List<RecurringTransaction> findByUserId(Long userId);

    @Lock(LockModeType.PESSIMISTIC_WRITE)
    @Query("SELECT r FROM RecurringTransaction r WHERE r.isActive = true AND r.nextDueDate <= :currentDate")
    List<RecurringTransaction> findOverdueTemplatesForUpdate(@Param("currentDate") LocalDate currentDate);
}


