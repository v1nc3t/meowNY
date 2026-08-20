package com.meowny.server.service;

import com.meowny.server.dto.user.CreateUserRequest;
import com.meowny.server.dto.user.UpdateUserRequest;
import com.meowny.server.dto.user.UserResponse;
import com.meowny.server.entity.User;
import com.meowny.server.exception.ResourceConflictException;
import com.meowny.server.repository.UserRepository;
import com.meowny.server.security.CurrentUserService;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.InjectMocks;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;
import org.springframework.security.crypto.password.PasswordEncoder;

import java.util.Optional;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class UserServiceTest {

    @Mock
    private UserRepository userRepository;

    @Mock
    private PasswordEncoder passwordEncoder;

    @Mock
    private CurrentUserService currentUserService;

    @InjectMocks
    private UserService userService;

    private User currentUser;

    @BeforeEach
    void setUp() {
        currentUser = new User();
        currentUser.setId(1L);
        currentUser.setEmail("same@example.com");
    }

    @Test
    @DisplayName("getCurrentUserProfile: Should return the authenticated user's profile")
    void getCurrentUserProfile_ReturnsResponse() {
        when(currentUserService.getCurrentUser()).thenReturn(currentUser);

        UserResponse response = userService.getCurrentUserProfile();

        assertThat(response.id()).isEqualTo(1L);
        verify(currentUserService).getCurrentUser();
    }

    @Test
    @DisplayName("createUser: Should create user successfully when email and username are unique")
    void createUser_ValidRequest_CreatesUser() {
        CreateUserRequest request = new CreateUserRequest(
                "John", "Doe", "john@example.com", "johndoe", "securepass123");
        User savedUser = new User();
        savedUser.setId(1L);
        savedUser.setEmail(request.email());

        when(userRepository.findUserByEmailIgnoreCase("john@example.com")).thenReturn(Optional.empty());
        when(userRepository.findUserByUsername(request.username())).thenReturn(Optional.empty());
        when(passwordEncoder.encode(request.password())).thenReturn("hashed_pass");
        when(userRepository.save(any(User.class))).thenReturn(savedUser);

        UserResponse response = userService.createUser(request);

        assertThat(response).isNotNull();
        verify(userRepository).save(any(User.class));
    }

    @Test
    @DisplayName("createUser: Should throw ResourceConflictException when email is taken")
    void createUser_EmailExists_ThrowsResourceConflictException() {
        CreateUserRequest request = new CreateUserRequest(
                "John", "Doe", "taken@example.com", "johndoe", "securepass123");
        when(userRepository.findUserByEmailIgnoreCase("taken@example.com")).thenReturn(Optional.of(new User()));

        assertThatThrownBy(() -> userService.createUser(request))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("Registration failed. Check your details and try again.");

        verify(userRepository, never()).save(any());
    }

    @Test
    @DisplayName("createUser: Should throw ResourceConflictException when username is taken")
    void createUser_UsernameExists_ThrowsResourceConflictException() {
        CreateUserRequest request = new CreateUserRequest(
                "John", "Doe", "john@example.com", "taken_user", "securepass123");
        when(userRepository.findUserByEmailIgnoreCase("john@example.com")).thenReturn(Optional.empty());
        when(userRepository.findUserByUsername(request.username())).thenReturn(Optional.of(new User()));

        assertThatThrownBy(() -> userService.createUser(request))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("Registration failed. Check your details and try again.");

        verify(userRepository, never()).save(any());
    }

    @Test
    @DisplayName("createUser: Should reject registration when email differs only by casing")
    void createUser_EmailExistsDifferentCasing_ThrowsResourceConflictException() {
        CreateUserRequest request = new CreateUserRequest(
                "John", "Doe", "Taken@Example.com", "johndoe", "securepass123");
        when(userRepository.findUserByEmailIgnoreCase("taken@example.com")).thenReturn(Optional.of(new User()));

        assertThatThrownBy(() -> userService.createUser(request))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("Registration failed. Check your details and try again.");

        verify(userRepository, never()).save(any());
    }

    @Test
    @DisplayName("updateCurrentUser: Should update names without checking uniqueness if email is unchanged")
    void updateCurrentUser_EmailUnchanged_UpdatesSuccessfully() {
        UpdateUserRequest request = new UpdateUserRequest("UpdatedFirst", "UpdatedLast", "same@example.com");
        when(currentUserService.getCurrentUser()).thenReturn(currentUser);
        when(userRepository.save(any(User.class))).thenReturn(currentUser);

        UserResponse response = userService.updateCurrentUser(request);

        assertThat(response).isNotNull();
        verify(userRepository, never()).findUserByEmailIgnoreCase(any());
        verify(userRepository).save(currentUser);
    }

    @Test
    @DisplayName("updateCurrentUser: Should check uniqueness and update email when email changes and is free")
    void updateCurrentUser_EmailChangedAndFree_UpdatesSuccessfully() {
        UpdateUserRequest request = new UpdateUserRequest("John", "Doe", "new@example.com");
        currentUser.setEmail("old@example.com");

        when(currentUserService.getCurrentUser()).thenReturn(currentUser);
        when(userRepository.findUserByEmailIgnoreCase("new@example.com")).thenReturn(Optional.empty());
        when(userRepository.save(any(User.class))).thenReturn(currentUser);

        UserResponse response = userService.updateCurrentUser(request);

        assertThat(response).isNotNull();
        verify(userRepository).findUserByEmailIgnoreCase("new@example.com");
        verify(userRepository).save(currentUser);
    }

    @Test
    @DisplayName("updateCurrentUser: Should throw ResourceConflictException when changing to an already registered email")
    void updateCurrentUser_EmailChangedAndTaken_ThrowsResourceConflictException() {
        UpdateUserRequest request = new UpdateUserRequest("John", "Doe", "taken@example.com");
        currentUser.setEmail("old@example.com");

        when(currentUserService.getCurrentUser()).thenReturn(currentUser);
        when(userRepository.findUserByEmailIgnoreCase("taken@example.com")).thenReturn(Optional.of(new User()));

        assertThatThrownBy(() -> userService.updateCurrentUser(request))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("This email address is already registered to another user.");

        verify(userRepository, never()).save(any());
    }

    @Test
    @DisplayName("deleteCurrentUser: Should delete the authenticated user")
    void deleteCurrentUser_DeletesSuccessfully() {
        when(currentUserService.getCurrentUser()).thenReturn(currentUser);

        userService.deleteCurrentUser();

        verify(userRepository).delete(currentUser);
    }
}
