import { createRouter, createWebHistory, type RouteRecordRaw } from 'vue-router'
import { useAuthStore } from '@/stores/auth'

const routes: RouteRecordRaw[] = [
  {
    path: '/login',
    name: 'login',
    component: () => import('@/views/auth/LoginView.vue'),
    meta: { public: true }
  },
  {
    path: '/register',
    name: 'register',
    component: () => import('@/views/auth/RegisterView.vue'),
    meta: { public: true }
  },
  {
    path: '/',
    component: () => import('@/layouts/AppLayout.vue'),
    children: [
      {
        path: '',
        name: 'home',
        component: () => import('@/views/HomeView.vue')
      },
      {
        path: 'practice',
        name: 'practice',
        component: () => import('@/views/quiz/PracticeView.vue')
      },
      {
        path: 'quiz/:sessionId',
        name: 'quiz-session',
        component: () => import('@/views/quiz/QuizSessionView.vue'),
        props: true
      },
      {
        path: 'chapter/:chapterId',
        name: 'chapter',
        component: () => import('@/views/quiz/ChapterView.vue'),
        props: true
      },
      {
        path: 'battle',
        name: 'battle',
        component: () => import('@/views/battle/BattleLobbyView.vue')
      },
      {
        path: 'battle/:roomId',
        name: 'battle-room',
        component: () => import('@/views/battle/BattleRoomView.vue'),
        props: true
      },
      {
        path: 'recommendation',
        name: 'recommendation',
        component: () => import('@/views/social/RecommendationView.vue')
      },
      {
        path: 'leaderboard',
        name: 'leaderboard',
        component: () => import('@/views/social/LeaderboardView.vue')
      },
      {
        path: 'profile',
        name: 'profile',
        component: () => import('@/views/social/ProfileView.vue')
      },
      {
        path: 'admin',
        name: 'admin',
        component: () => import('@/views/admin/AdminView.vue')
      }
    ]
  },
  {
    path: '/:pathMatch(.*)*',
    name: 'not-found',
    component: () => import('@/views/NotFoundView.vue'),
    meta: { public: true }
  }
]

const router = createRouter({
  history: createWebHistory(),
  routes,
  scrollBehavior(_to, _from, savedPosition) {
    return savedPosition ?? { top: 0 }
  }
})

router.beforeEach((to) => {
  const auth = useAuthStore()
  if (to.meta.public) return true
  if (!auth.isAuthenticated) {
    return { name: 'login', query: { redirect: to.fullPath } }
  }
  return true
})

export default router
