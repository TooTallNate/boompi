import React, { useState, useLayoutEffect, useRef } from 'react';

// CSS
import styles from '@styles/marquee.module.css';

export default function Marquee({ children }: any) {
	const content = useRef<HTMLSpanElement>(null);
	const outer = useRef<HTMLDivElement>(null);
	const [width, setWidth] = useState(0);

	useLayoutEffect(() => {
		if (!outer.current || !content.current) return;
		const { width: outerWidth } = outer.current.getBoundingClientRect();
		const { width: contentWidth } = content.current.getBoundingClientRect();
		setWidth(contentWidth <= outerWidth ? 0 : contentWidth);
	}, [children]);

	const innerStyle: React.CSSProperties = {};
	const outerClasses = [styles.outer];
	const innerClasses = [styles.inner];
	const spacerWidth = 200;

	let leftSpacer = null;
	let runningComponents = null;
	if (width > 0) {
		const copy = React.Children.map(children, (el) => {
			if (React.isValidElement(el)) {
				return React.cloneElement(el);
			}
			return el;
		});
		runningComponents = (
			<>
				<div
					style={{
						width: `${spacerWidth}px`,
						display: 'inline-block',
					}}
				/>
				<span className={styles.copy}>{copy}</span>
			</>
		);
		leftSpacer = (
			<div
				style={{
					width: `${20}px`,
					display: 'inline-block',
				}}
			/>
		);
		innerStyle.transform = `translateX(-${width + spacerWidth}px)`;
		innerClasses.push(styles.active);
		outerClasses.push(styles.outerActive);
	}

	return (
		<div ref={outer} className={outerClasses.join(' ')}>
			<div className={innerClasses.join(' ')} style={innerStyle}>
				{leftSpacer}
				<span ref={content} className={styles.source}>
					{children}
				</span>
				{runningComponents}
			</div>
		</div>
	);
}
