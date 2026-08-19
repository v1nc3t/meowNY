package com.meowny.server.support;

import com.meowny.server.entity.User;
import com.meowny.server.exception.ResourceNotFoundException;
import com.meowny.server.security.CurrentUserService;

import static org.mockito.ArgumentMatchers.anyLong;
import static org.mockito.Mockito.lenient;

public final class TestCurrentUserSupport {

    private TestCurrentUserSupport() {
    }

    public static void stubCurrentUser(CurrentUserService currentUserService, User currentUser) {
        lenient().when(currentUserService.getCurrentUser()).thenReturn(currentUser);
        lenient().doAnswer(invocation -> {
            Long ownerId = invocation.getArgument(0);
            if (!ownerId.equals(currentUser.getId())) {
                throw new ResourceNotFoundException();
            }
            return null;
        }).when(currentUserService).requireOwnedByCurrentUser(anyLong());
    }
}
