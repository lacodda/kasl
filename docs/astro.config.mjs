// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// Served from the custom domain in ./public/CNAME, so the site sits at the
// root - no `base` path, unlike a github.io project site.
export default defineConfig({
	site: 'https://kasl.lacodda.com',
	integrations: [
		starlight({
			title: 'kasl',
			description:
				'Key Activity Synchronization and Logging: a CLI that watches your activity, turns it into workdays, pauses and tasks, and files the report for you.',
			logo: {
				src: './src/assets/logo.svg',
				alt: 'kasl',
			},
			favicon: '/favicon.svg',
			customCss: ['./src/styles/brand.css'],
			head: [
				{ tag: 'link', attrs: { rel: 'apple-touch-icon', href: '/apple-touch-icon.png' } },
				{
					tag: 'meta',
					attrs: { property: 'og:image', content: 'https://raw.githubusercontent.com/lacodda/kasl/main/assets/social-preview.png' },
				},
				{ tag: 'meta', attrs: { name: 'twitter:card', content: 'summary_large_image' } },
			],
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/lacodda/kasl' }],
			editLink: {
				baseUrl: 'https://github.com/lacodda/kasl/edit/main/docs/',
			},
			sidebar: [
				{ label: 'Getting Started', slug: 'getting-started' },
				{
					label: 'Guides',
					items: [{ autogenerate: { directory: 'guides' } }],
				},
				{
					label: 'Concepts',
					items: [{ autogenerate: { directory: 'concepts' } }],
				},
				{
					label: 'Reference',
					items: [{ autogenerate: { directory: 'reference' } }],
				},
			],
		}),
	],
});
